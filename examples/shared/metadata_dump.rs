use anyhow::Result;
use astro_metadata::AstroMetadata;
use std::env;
use std::path::Path;
use std::process;

/// Run a metadata dump and append format-specific diagnostics after the
/// semantic report.
pub fn run_metadata_dump_with_postscript<F, G>(
    format_name: &str,
    usage_arg: &str,
    raw_header_title: &str,
    extract_metadata_from_path: F,
    postscript: G,
) where
    F: Fn(&Path) -> Result<AstroMetadata>,
    G: Fn(&Path) -> Result<()>,
{
    let args: Vec<String> = env::args().collect();

    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help") {
        println!("Usage: {} {}", args[0], usage_arg);
        return;
    }

    if args.len() != 2 {
        eprintln!("Usage: {} {}", args[0], usage_arg);
        process::exit(1);
    }

    let file_path = Path::new(&args[1]);

    match extract_metadata_from_path(file_path) {
        Ok(metadata) => {
            print_metadata_report(format_name, raw_header_title, file_path, &metadata);
            if let Err(err) = postscript(file_path) {
                eprintln!("Error writing metadata diagnostics: {}", err);
                process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("Error extracting metadata: {}", err);
            process::exit(1);
        }
    }
}

fn print_metadata_report(
    format_name: &str,
    raw_header_title: &str,
    file_path: &Path,
    metadata: &AstroMetadata,
) {
    println!("{} Metadata for: {}", format_name, file_path.display());
    println!("\n=== Equipment Information ===");
    if let Some(telescope) = &metadata.equipment.telescope_name {
        println!("Telescope: {}", telescope);
    }
    if let Some(focal_length) = metadata.equipment.focal_length {
        println!("Focal Length: {} mm", focal_length);
    }
    if let Some(aperture) = metadata.equipment.aperture {
        println!("Aperture: {} mm", aperture);
    }
    if let Some(focal_ratio) = metadata.equipment.focal_ratio {
        println!("Focal Ratio: f/{:.1}", focal_ratio);
    }

    println!("\n=== Detector Information ===");
    if let Some(camera) = &metadata.detector.camera_name {
        println!("Camera: {}", camera);
    }
    println!(
        "Image Dimensions: {}x{} pixels",
        metadata.detector.width, metadata.detector.height
    );
    if metadata.detector.binning_x > 1 || metadata.detector.binning_y > 1 {
        println!(
            "Binning: {}x{}",
            metadata.detector.binning_x, metadata.detector.binning_y
        );
    }
    if let Some(temp) = metadata.detector.temperature {
        println!("Sensor Temperature: {:.1} °C", temp);
    }

    println!("\n=== Exposure Information ===");
    if let Some(object) = &metadata.exposure.object_name {
        println!("Object: {}", object);
    }
    if let Some(exp_time) = metadata.exposure.exposure_time {
        println!("Exposure Time: {:.2} seconds", exp_time);
    }
    if let Some(frame_type) = &metadata.exposure.frame_type {
        println!("Frame Type: {}", frame_type);
    }
    if let Some(date_obs) = metadata.exposure.date_obs {
        println!(
            "Observation Date: {}",
            date_obs.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }
    if let (Some(ra), Some(dec)) = (metadata.exposure.ra, metadata.exposure.dec) {
        println!("Coordinates: RA={:.6}°, Dec={:.6}°", ra, dec);
    }

    println!("\n=== Filter Information ===");
    if let Some(filter) = &metadata.filter.name {
        println!("Filter: {}", filter);
    }

    if let Some(mount) = &metadata.mount {
        println!("\n=== Mount Information ===");
        if let Some(pier_side) = &mount.pier_side {
            println!("Pier Side: {}", pier_side);
        }
        if let (Some(lat), Some(long)) = (mount.latitude, mount.longitude) {
            println!("Location: Lat={:.6}°, Long={:.6}°", lat, long);
        }
        if let Some(guide_rms) = mount.guide_rms {
            println!("Guide RMS: {:.2} pixels", guide_rms);
        }
    }

    if let Some(env) = &metadata.environment {
        println!("\n=== Environment Information ===");
        if let Some(temp) = env.ambient_temp {
            println!("Ambient Temperature: {:.1} °C", temp);
        }
        if let Some(humidity) = env.humidity {
            println!("Humidity: {:.1}%", humidity);
        }
        if let Some(sqm) = env.sqm {
            println!("Sky Quality: {:.2} mag/arcsec²", sqm);
        }
        if let Some(software_version) = &env.software_version {
            println!("Software Version: {}", software_version);
        }
    }

    println!("\n=== Calculated Values ===");
    if let Some(plate_scale) = metadata.plate_scale() {
        println!("Plate Scale: {:.3} arcsec/pixel", plate_scale);
    }
    if let Some((width, height)) = metadata.field_of_view() {
        println!("Field of View: {:.2}' × {:.2}' (arcmin)", width, height);
    }

    if let Some(xisf) = &metadata.xisf {
        println!("\n=== XISF Information ===");
        println!("Format Version: {}", xisf.version);
        if let Some(creator) = &xisf.creator {
            println!("Creator: {}", creator);
        }
        if let Some(creation_time) = xisf.creation_time {
            println!(
                "Creation Time: {}",
                creation_time.format("%Y-%m-%d %H:%M:%S UTC")
            );
        }
        if let Some(block_alignment) = xisf.block_alignment {
            println!("Block Alignment: {} bytes", block_alignment);
        }
    }

    if let Some(color_management) = &metadata.color_management {
        println!("\n=== Color Management ===");
        if let Some(color_space) = &color_management.color_space {
            println!("Color Space: {}", color_space);
        }
        if color_management.icc_profile.is_some() {
            println!("ICC Profile: present");
        }
        if let Some(display_function) = &color_management.display_function {
            if let Some(function_type) = &display_function.function_type {
                println!("Display Function: {}", function_type);
            }
            if !display_function.parameters.is_empty() {
                let mut parameters: Vec<_> = display_function.parameters.iter().collect();
                parameters.sort_by(|(left, _), (right, _)| left.cmp(right));
                let formatted = parameters
                    .into_iter()
                    .map(|(key, value)| format!("{}={}", key, value))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("Display Parameters: {}", formatted);
            }
        }
    }

    if !metadata.attachments.is_empty() {
        println!("\n=== XISF Attachments ===");
        for (index, attachment) in metadata.attachments.iter().enumerate() {
            let label = if attachment.id.is_empty() {
                format!("attachment{}", index)
            } else {
                attachment.id.clone()
            };

            println!(
                "[{}] {} | geometry={} | sample={} | bits={}",
                index,
                label,
                attachment.geometry,
                attachment.sample_format,
                attachment.bits_per_sample
            );

            if let Some(compression) = &attachment.compression {
                println!("    Compression: {}", compression);
            }
            if let Some(checksum_type) = &attachment.checksum_type {
                println!("    Checksum Type: {}", checksum_type);
            }
            if let Some(checksum) = &attachment.checksum {
                println!("    Checksum: {}", checksum);
            }
        }
    }

    println!("\n=== {} ===", raw_header_title);
    for card in &metadata.raw_header_cards {
        match (&card.value, &card.comment, &card.raw_card) {
            (Some(value), Some(comment), _) => {
                println!(
                    "[HDU {} #{:03}] {} = {} / {}",
                    card.hdu_index, card.card_index, card.keyword, value, comment
                );
            }
            (Some(value), None, _) => {
                println!(
                    "[HDU {} #{:03}] {} = {}",
                    card.hdu_index, card.card_index, card.keyword, value
                );
            }
            (None, Some(comment), _) => {
                println!(
                    "[HDU {} #{:03}] {} / {}",
                    card.hdu_index, card.card_index, card.keyword, comment
                );
            }
            (None, None, Some(raw_card)) => {
                println!(
                    "[HDU {} #{:03}] {}",
                    card.hdu_index, card.card_index, raw_card
                );
            }
            (None, None, None) => {
                println!(
                    "[HDU {} #{:03}] {}",
                    card.hdu_index, card.card_index, card.keyword
                );
            }
        }
    }
}
