use anyhow::{bail, Context, Result};
use csv::Writer;
use ravensky_astro::metadata::{fits_parser, xisf_parser, AstroMetadata};
use rayon::prelude::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const ELEMENT_CSV_NAME: &str = "metadata_stats_elements.csv";
const VALUE_CSV_NAME: &str = "metadata_stats_values.csv";
const BINARY_PLACEHOLDER: &str = "<binary payload omitted>";
const PERCENT_PRECISION: usize = 6;
const MAX_FAILURES_TO_DISPLAY: usize = 10;

pub fn run_metadata_stats() -> Result<()> {
    let raw_args: Vec<OsString> = env::args_os().collect();
    let program = raw_args
        .first()
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_else(|| "metadata_stats".to_owned());

    let command = match parse_args(raw_args) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("{err}");
            eprintln!("{}", usage(&program));
            std::process::exit(1);
        }
    };

    match command {
        Command::Help => {
            println!("{}", usage(&program));
            Ok(())
        }
        Command::Run(options) => run_with_options(&options),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Run(CliOptions),
}

#[derive(Debug, PartialEq, Eq)]
struct CliOptions {
    input_path: PathBuf,
    count_only: bool,
}

#[derive(Debug, Default)]
struct DiscoveryResult {
    image_paths: Vec<PathBuf>,
    skipped_permission_paths: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct StatsAccumulator {
    files_processed: usize,
    skipped_permission_paths: Vec<PathBuf>,
    failures: Vec<FileFailure>,
    elements: HashMap<String, ElementStats>,
}

#[derive(Debug)]
struct FileFailure {
    path: PathBuf,
    message: String,
}

#[derive(Debug, Default)]
struct ElementStats {
    files_with_element: usize,
    value_counts: HashMap<String, usize>,
}

#[derive(Debug, Default)]
struct FlattenedMetadata {
    present_elements: HashSet<String>,
    value_presence: HashMap<String, HashSet<String>>,
}

fn run_with_options(options: &CliOptions) -> Result<()> {
    let discovery = collect_image_paths(&options.input_path)?;

    if options.count_only {
        println!("{}", discovery.image_paths.len());
        return Ok(());
    }

    if discovery.image_paths.is_empty() {
        if discovery.skipped_permission_paths.is_empty() {
            bail!(
                "No FITS or XISF files were found under {}",
                options.input_path.display()
            );
        }

        let empty_stats = StatsAccumulator::default();
        write_csv_reports(&empty_stats)?;
        print_summary(&empty_stats, 0, &discovery.skipped_permission_paths);
        return Ok(());
    }

    let stats = analyze_image_paths(&discovery.image_paths);

    if stats.files_processed == 0 && stats.failures.is_empty() {
        write_csv_reports(&stats)?;
        print_summary(
            &stats,
            discovery.image_paths.len(),
            &discovery.skipped_permission_paths,
        );
        return Ok(());
    }

    if stats.files_processed == 0 {
        bail!(
            "Found {} supported files, but none could be processed successfully",
            discovery.image_paths.len()
        );
    }

    write_csv_reports(&stats)?;
    print_summary(
        &stats,
        discovery.image_paths.len(),
        &discovery.skipped_permission_paths,
    );

    Ok(())
}

fn parse_args<I>(args: I) -> Result<Command>
where
    I: IntoIterator<Item = OsString>,
{
    let mut positional_args = Vec::new();
    let mut count_only = false;

    for arg in args.into_iter().skip(1) {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("--count") => count_only = true,
            Some(flag) if flag.starts_with('-') => bail!("Unknown option: {flag}"),
            _ => positional_args.push(arg),
        }
    }

    if positional_args.len() != 1 {
        bail!("Expected exactly one input path");
    }

    Ok(Command::Run(CliOptions {
        input_path: PathBuf::from(&positional_args[0]),
        count_only,
    }))
}

fn usage(program: &str) -> String {
    format!("Usage: {program} [--count] <path>")
}

fn collect_image_paths(root: &Path) -> Result<DiscoveryResult> {
    if !root.exists() {
        bail!("Input path does not exist: {}", root.display());
    }

    if root.is_file() {
        if is_supported_image_path(root) {
            return Ok(DiscoveryResult {
                image_paths: vec![root.to_path_buf()],
                skipped_permission_paths: Vec::new(),
            });
        }

        bail!(
            "Input file is not a supported FITS or XISF image: {}",
            root.display()
        );
    }

    let mut discovery = DiscoveryResult::default();

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) if is_walkdir_permission_denied(&err) => {
                if let Some(path) = err.path() {
                    discovery.skipped_permission_paths.push(path.to_path_buf());
                }
                continue;
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed while traversing {}", root.display()));
            }
        };

        if entry.file_type().is_file() && is_supported_image_path(entry.path()) {
            discovery.image_paths.push(entry.into_path());
        }
    }

    discovery.image_paths.sort_unstable();
    discovery.skipped_permission_paths.sort_unstable();
    discovery.skipped_permission_paths.dedup();
    Ok(discovery)
}

fn is_supported_image_path(path: &Path) -> bool {
    matches!(
        normalized_extension(path).as_deref(),
        Some("fits" | "fit" | "fts" | "xisf")
    )
}

fn normalized_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|extension| extension.to_ascii_lowercase())
}

fn analyze_image_paths(paths: &[PathBuf]) -> StatsAccumulator {
    // Rayon fold/reduce keeps aggregation thread-local while files are being parsed,
    // which avoids lock contention on the hot path and only merges summaries once.
    paths
        .par_iter()
        .fold(StatsAccumulator::default, |mut local_stats, path| {
            local_stats.process_path(path);
            local_stats
        })
        .reduce(StatsAccumulator::default, |mut combined, partial| {
            combined.merge(partial);
            combined
        })
}

impl StatsAccumulator {
    fn process_path(&mut self, path: &Path) {
        match extract_metadata_for_path(path).and_then(|metadata| flatten_metadata(&metadata)) {
            Ok(flattened) => {
                self.files_processed += 1;
                self.record_flattened(flattened);
            }
            Err(err) if is_permission_denied_error(&err) => {
                self.skipped_permission_paths.push(path.to_path_buf());
            }
            Err(err) => self.failures.push(FileFailure {
                path: path.to_path_buf(),
                message: err.to_string(),
            }),
        }
    }

    fn record_flattened(&mut self, flattened: FlattenedMetadata) {
        for element in flattened.present_elements {
            self.elements.entry(element).or_default().files_with_element += 1;
        }

        // Each value is counted at most once per file so repeated attachments or
        // duplicate keywords do not overstate how common a value is across files.
        for (element, values) in flattened.value_presence {
            if is_expected_to_vary_per_exposure(&element) {
                continue;
            }

            let element_stats = self.elements.entry(element).or_default();

            for value in values {
                *element_stats.value_counts.entry(value).or_insert(0) += 1;
            }
        }
    }

    fn merge(&mut self, other: StatsAccumulator) {
        self.files_processed += other.files_processed;
        self.skipped_permission_paths
            .extend(other.skipped_permission_paths);
        self.failures.extend(other.failures);

        for (element, other_stats) in other.elements {
            let element_stats = self.elements.entry(element).or_default();
            element_stats.files_with_element += other_stats.files_with_element;

            for (value, count) in other_stats.value_counts {
                *element_stats.value_counts.entry(value).or_insert(0) += count;
            }
        }
    }
}

fn is_walkdir_permission_denied(error: &walkdir::Error) -> bool {
    error.io_error().is_some_and(is_permission_denied_io_error)
}

fn is_permission_denied_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<io::Error>()
            .is_some_and(is_permission_denied_io_error)
    })
}

fn is_permission_denied_io_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied || matches!(error.raw_os_error(), Some(1 | 13))
}

fn extract_metadata_for_path(path: &Path) -> Result<AstroMetadata> {
    match normalized_extension(path).as_deref() {
        Some("fits" | "fit" | "fts") => fits_parser::extract_metadata_from_path(path)
            .with_context(|| format!("Failed to extract FITS metadata from {}", path.display())),
        Some("xisf") => xisf_parser::extract_metadata_from_path(path)
            .with_context(|| format!("Failed to extract XISF metadata from {}", path.display())),
        _ => bail!("Unsupported image format: {}", path.display()),
    }
}

fn flatten_metadata(metadata: &AstroMetadata) -> Result<FlattenedMetadata> {
    let mut sanitized_metadata = metadata.clone();
    sanitized_metadata.raw_header_cards.clear();
    sanitized_metadata.raw_headers.clear();

    let has_icc_profile = sanitized_metadata
        .color_management
        .as_ref()
        .and_then(|color_management| color_management.icc_profile.as_ref())
        .is_some();

    if let Some(color_management) = &mut sanitized_metadata.color_management {
        color_management.icc_profile = None;
    }

    // We intentionally flatten the semantic metadata object instead of the raw headers
    // so the output stays stable across formats and focuses on meaning, not storage details.
    let json_value = serde_json::to_value(sanitized_metadata)
        .context("Failed to serialize metadata into a traversal-friendly value")?;

    let mut flattened = FlattenedMetadata::default();
    flatten_json_value(None, &json_value, &mut flattened);

    if has_icc_profile {
        flattened.record_scalar(
            "color_management.icc_profile".to_owned(),
            BINARY_PLACEHOLDER.to_owned(),
            true,
        );
    }

    Ok(flattened)
}

fn flatten_json_value(path: Option<String>, value: &Value, flattened: &mut FlattenedMetadata) {
    match value {
        Value::Null => {}
        Value::Bool(boolean) => {
            if let Some(path) = path {
                let track_values = should_track_values(&path);
                flattened.record_scalar(path, boolean.to_string(), track_values);
            }
        }
        Value::Number(number) => {
            if let Some(path) = path {
                let track_values = should_track_values(&path);
                flattened.record_scalar(path, number.to_string(), track_values);
            }
        }
        Value::String(string) => {
            if let Some(path) = path {
                let trimmed = string.trim();
                if !trimmed.is_empty() {
                    let track_values = should_track_values(&path);
                    flattened.record_scalar(path, trimmed.to_owned(), track_values);
                }
            }
        }
        Value::Array(values) => {
            if values.is_empty() {
                return;
            }

            let Some(path) = path else {
                return;
            };

            let item_path = format!("{path}[]");
            for value in values {
                flatten_json_value(Some(item_path.clone()), value, flattened);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "raw_header_cards" | "raw_headers") {
                    continue;
                }

                let child_path = match &path {
                    Some(parent) => format!("{parent}.{key}"),
                    None => key.clone(),
                };

                flatten_json_value(Some(child_path), value, flattened);
            }
        }
    }
}

fn should_track_values(element: &str) -> bool {
    !is_expected_to_vary_per_exposure(element)
}

fn is_expected_to_vary_per_exposure(element: &str) -> bool {
    // Presence still matters for these fields, but their concrete values are usually
    // a moving measurement, coordinate, counter, or checksum rather than a reusable
    // category we want to summarize across a directory of exposures.
    matches!(
        element,
        "equipment.focuser_position"
            | "equipment.focuser_temperature"
            | "detector.temperature"
            | "detector.temp_setpoint"
            | "detector.cooler_power"
            | "exposure.ra"
            | "exposure.dec"
            | "exposure.date_obs"
            | "exposure.session_date"
            | "exposure.frame_number"
            | "exposure.dither_offset_x"
            | "exposure.dither_offset_y"
            | "mount.guide_rms"
            | "mount.peak_ra_error"
            | "mount.peak_dec_error"
            | "environment.ambient_temp"
            | "environment.humidity"
            | "environment.dew_heater_power"
            | "environment.voltage"
            | "environment.current"
            | "environment.sqm"
            | "wcs.crpix1"
            | "wcs.crpix2"
            | "wcs.crval1"
            | "wcs.crval2"
            | "wcs.cd1_1"
            | "wcs.cd1_2"
            | "wcs.cd2_1"
            | "wcs.cd2_2"
            | "wcs.crota2"
            | "wcs.airmass"
            | "wcs.altitude"
            | "wcs.azimuth"
            | "xisf.creation_time"
            | "attachments[].checksum"
    )
}

impl FlattenedMetadata {
    fn record_scalar(&mut self, element: String, value: String, track_value: bool) {
        self.present_elements.insert(element.clone());

        if track_value {
            self.value_presence
                .entry(element)
                .or_default()
                .insert(value);
        }
    }
}

fn write_csv_reports(stats: &StatsAccumulator) -> Result<()> {
    let element_file = File::create(ELEMENT_CSV_NAME)
        .with_context(|| format!("Failed to create {ELEMENT_CSV_NAME}"))?;
    let value_file = File::create(VALUE_CSV_NAME)
        .with_context(|| format!("Failed to create {VALUE_CSV_NAME}"))?;

    write_element_csv(BufWriter::new(element_file), stats)?;
    write_value_csv(BufWriter::new(value_file), stats)?;

    Ok(())
}

fn write_element_csv<W: Write>(writer: W, stats: &StatsAccumulator) -> Result<()> {
    let mut csv_writer = Writer::from_writer(writer);
    csv_writer.write_record(["metadata_element", "percent_of_files"])?;

    let mut elements: Vec<_> = stats.elements.iter().collect();
    elements.sort_unstable_by_key(|(left, _)| *left);

    for (element, element_stats) in elements {
        let percentage = percentage(element_stats.files_with_element, stats.files_processed);
        csv_writer.write_record([element.as_str(), &format_percentage(percentage)])?;
    }

    csv_writer.flush()?;
    Ok(())
}

fn write_value_csv<W: Write>(writer: W, stats: &StatsAccumulator) -> Result<()> {
    let mut csv_writer = Writer::from_writer(writer);
    csv_writer.write_record(["metadata_element", "value", "percent_of_element"])?;

    let mut elements: Vec<_> = stats.elements.iter().collect();
    elements.sort_unstable_by_key(|(left, _)| *left);

    for (element, element_stats) in elements {
        if is_expected_to_vary_per_exposure(element)
            || element_stats.files_with_element == 0
            || element_stats.value_counts.is_empty()
        {
            continue;
        }

        let mut values: Vec<_> = element_stats.value_counts.iter().collect();
        values.sort_unstable_by_key(|(left, _)| *left);

        for (value, count) in values {
            let percentage = percentage(*count, element_stats.files_with_element);
            csv_writer.write_record([
                element.as_str(),
                value.as_str(),
                &format_percentage(percentage),
            ])?;
        }
    }

    csv_writer.flush()?;
    Ok(())
}

fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }

    (numerator as f64 / denominator as f64) * 100.0
}

fn format_percentage(percentage: f64) -> String {
    format!("{percentage:.precision$}", precision = PERCENT_PRECISION)
}

fn print_summary(
    stats: &StatsAccumulator,
    files_discovered: usize,
    discovery_permission_skips: &[PathBuf],
) {
    println!(
        "Processed {} of {} discovered FITS/XISF files.",
        stats.files_processed, files_discovered
    );
    println!("Wrote {ELEMENT_CSV_NAME} and {VALUE_CSV_NAME}.");

    print_permission_skips(
        "Skipped paths during discovery due to permission restrictions",
        discovery_permission_skips,
    );
    print_permission_skips(
        "Skipped files during metadata extraction due to permission restrictions",
        &stats.skipped_permission_paths,
    );

    if stats.failures.is_empty() {
        return;
    }

    eprintln!(
        "Skipped {} files due to metadata extraction errors.",
        stats.failures.len()
    );

    let mut failures: Vec<_> = stats.failures.iter().collect();
    failures.sort_unstable_by(|left, right| left.path.cmp(&right.path));

    for failure in failures.into_iter().take(MAX_FAILURES_TO_DISPLAY) {
        eprintln!("  {}: {}", failure.path.display(), failure.message);
    }

    if stats.failures.len() > MAX_FAILURES_TO_DISPLAY {
        eprintln!(
            "  ... and {} more failures",
            stats.failures.len() - MAX_FAILURES_TO_DISPLAY
        );
    }
}

fn print_permission_skips(label: &str, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }

    eprintln!("{label}: {}.", paths.len());
    let skipped_count = paths.len();

    let mut paths: Vec<_> = paths.iter().collect();
    paths.sort_unstable();

    for path in paths.iter().take(MAX_FAILURES_TO_DISPLAY) {
        eprintln!("  {}", path.display());
    }

    if skipped_count > MAX_FAILURES_TO_DISPLAY {
        eprintln!("  ... and {} more", skipped_count - MAX_FAILURES_TO_DISPLAY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ravensky_astro::io::fits::FitsHeaderCard;
    use ravensky_astro::metadata::types::{AttachmentInfo, ColorManagement};

    #[test]
    fn parse_args_supports_count_before_or_after_path() {
        let options_before = parse_args(vec![
            OsString::from("metadata_stats"),
            OsString::from("--count"),
            OsString::from("tests/data"),
        ])
        .unwrap();
        let options_after = parse_args(vec![
            OsString::from("metadata_stats"),
            OsString::from("tests/data"),
            OsString::from("--count"),
        ])
        .unwrap();

        assert_eq!(
            options_before,
            Command::Run(CliOptions {
                input_path: PathBuf::from("tests/data"),
                count_only: true,
            })
        );
        assert_eq!(options_before, options_after);
    }

    #[test]
    fn flatten_metadata_skips_raw_headers_and_temporal_value_counts() {
        let mut metadata = AstroMetadata::default();
        metadata.equipment.telescope_name = Some("RC8".to_owned());
        metadata.equipment.focuser_position = Some(11876);
        metadata.exposure.date_obs = Some(Utc.with_ymd_and_hms(2024, 9, 4, 8, 39, 13).unwrap());
        metadata.exposure.ra = Some(83.822_08);
        metadata.exposure.dec = Some(-5.391_11);
        metadata
            .raw_headers
            .insert("DATE-OBS".to_owned(), "2024-09-04T08:39:13.204".to_owned());
        metadata.raw_header_cards.push(FitsHeaderCard {
            keyword: "DATE-OBS".to_owned(),
            value: Some("2024-09-04T08:39:13.204".to_owned()),
            ..FitsHeaderCard::default()
        });
        metadata.color_management = Some(ColorManagement {
            color_space: Some("RGB".to_owned()),
            icc_profile: Some(vec![1, 2, 3]),
            ..ColorManagement::default()
        });

        let flattened = flatten_metadata(&metadata).unwrap();

        assert!(flattened
            .present_elements
            .contains("equipment.telescope_name"));
        assert!(flattened.present_elements.contains("exposure.date_obs"));
        assert!(flattened
            .present_elements
            .contains("color_management.icc_profile"));
        assert!(!flattened.present_elements.contains("raw_headers.DATE-OBS"));
        assert!(!flattened
            .present_elements
            .contains("raw_header_cards[].keyword"));
        assert_eq!(
            flattened.value_presence["equipment.telescope_name"],
            HashSet::from([String::from("RC8")])
        );
        assert!(flattened
            .present_elements
            .contains("equipment.focuser_position"));
        assert!(!flattened.value_presence.contains_key("exposure.date_obs"));
        assert!(!flattened
            .value_presence
            .contains_key("equipment.focuser_position"));
        assert!(!flattened.value_presence.contains_key("exposure.ra"));
        assert!(!flattened.value_presence.contains_key("exposure.dec"));
        assert_eq!(
            flattened.value_presence["color_management.icc_profile"],
            HashSet::from([String::from(BINARY_PLACEHOLDER)])
        );
    }

    #[test]
    fn variable_value_filter_matches_expected_per_exposure_fields() {
        assert!(is_expected_to_vary_per_exposure(
            "equipment.focuser_position"
        ));
        assert!(is_expected_to_vary_per_exposure("detector.temperature"));
        assert!(is_expected_to_vary_per_exposure("environment.ambient_temp"));
        assert!(is_expected_to_vary_per_exposure("mount.guide_rms"));
        assert!(is_expected_to_vary_per_exposure("wcs.crval1"));
        assert!(is_expected_to_vary_per_exposure("attachments[].checksum"));

        assert!(!is_expected_to_vary_per_exposure("detector.gain"));
        assert!(!is_expected_to_vary_per_exposure("detector.offset"));
        assert!(!is_expected_to_vary_per_exposure("filter.name"));
        assert!(!is_expected_to_vary_per_exposure("exposure.exposure_time"));
        assert!(!is_expected_to_vary_per_exposure(
            "equipment.telescope_name"
        ));
    }

    #[test]
    fn flatten_metadata_tracks_array_members_with_stable_paths() {
        let mut metadata = AstroMetadata::default();
        metadata.attachments.push(AttachmentInfo {
            id: "thumbnail".to_owned(),
            geometry: "64:64:1".to_owned(),
            sample_format: "UInt16".to_owned(),
            bits_per_sample: 16,
            ..AttachmentInfo::default()
        });

        let flattened = flatten_metadata(&metadata).unwrap();

        assert!(flattened.present_elements.contains("attachments[].id"));
        assert!(flattened
            .present_elements
            .contains("attachments[].geometry"));
        assert!(flattened
            .present_elements
            .contains("attachments[].bits_per_sample"));
        assert_eq!(
            flattened.value_presence["attachments[].id"],
            HashSet::from([String::from("thumbnail")])
        );
    }

    #[test]
    fn record_flattened_counts_values_once_per_file() {
        let mut stats = StatsAccumulator::default();

        let mut first_file = FlattenedMetadata::default();
        first_file.record_scalar("filter.name".to_owned(), "Ha".to_owned(), true);
        first_file.record_scalar("filter.name".to_owned(), "Ha".to_owned(), true);
        stats.files_processed += 1;
        stats.record_flattened(first_file);

        let mut second_file = FlattenedMetadata::default();
        second_file.record_scalar("filter.name".to_owned(), "OIII".to_owned(), true);
        stats.files_processed += 1;
        stats.record_flattened(second_file);

        let filter_stats = &stats.elements["filter.name"];
        assert_eq!(filter_stats.files_with_element, 2);
        assert_eq!(filter_stats.value_counts["Ha"], 1);
        assert_eq!(filter_stats.value_counts["OIII"], 1);
    }

    #[test]
    fn record_flattened_does_not_store_variable_value_counts() {
        let mut stats = StatsAccumulator::default();

        let mut flattened = FlattenedMetadata::default();
        flattened.record_scalar(
            "equipment.focuser_position".to_owned(),
            "31077".to_owned(),
            true,
        );
        flattened.record_scalar(
            "equipment.telescope_name".to_owned(),
            "FRA400".to_owned(),
            true,
        );

        stats.files_processed += 1;
        stats.record_flattened(flattened);

        assert!(stats.elements.contains_key("equipment.focuser_position"));
        assert!(stats.elements["equipment.focuser_position"]
            .value_counts
            .is_empty());
        assert_eq!(
            stats.elements["equipment.telescope_name"].value_counts["FRA400"],
            1
        );
    }

    #[test]
    fn csv_writers_emit_expected_headers_and_percentages() {
        let mut stats = StatsAccumulator::default();
        stats.files_processed = 4;
        stats.elements.insert(
            "filter.name".to_owned(),
            ElementStats {
                files_with_element: 3,
                value_counts: HashMap::from([(String::from("Ha"), 2), (String::from("OIII"), 1)]),
            },
        );
        stats.elements.insert(
            "equipment.focuser_position".to_owned(),
            ElementStats {
                files_with_element: 4,
                value_counts: HashMap::from([(String::from("31077"), 4)]),
            },
        );

        let mut element_csv = Vec::new();
        write_element_csv(&mut element_csv, &stats).unwrap();
        let element_csv = String::from_utf8(element_csv).unwrap();
        assert!(element_csv.contains("metadata_element,percent_of_files"));
        assert!(element_csv.contains("equipment.focuser_position,100.000000"));
        assert!(element_csv.contains("filter.name,75.000000"));

        let mut value_csv = Vec::new();
        write_value_csv(&mut value_csv, &stats).unwrap();
        let value_csv = String::from_utf8(value_csv).unwrap();
        assert!(value_csv.contains("metadata_element,value,percent_of_element"));
        assert!(value_csv.contains("filter.name,Ha,66.666667"));
        assert!(value_csv.contains("filter.name,OIII,33.333333"));
        assert!(!value_csv.contains("equipment.focuser_position,31077"));
    }
}
