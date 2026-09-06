use anyhow::{bail, ensure, Context, Result};
use astro_bench::{run_sample, Encoding, FixtureSet, Manifest, Pattern, Recipe, Sample, Workload};
use serde::Serialize;
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const HELP: &str = "astro-bench run --output REPORT.json [options]
  --scratch DIR           Existing scratch parent (default: OS temporary directory)
  --encoding fits|xisf|zlib|zstd   (default: fits)
  --pattern noise|gradient       (default: noise)
  --workload read|structural|full (default: full)
  --width N --height N     Pixel dimensions (default: 2048 x 2048; <=64 MiB/image)
  --frames N              1..256 (default: 4)
  --seed N                Deterministic seed (default: 42)
  --disk-mib N            Scratch quota (default: 512; maximum: 8192)
  --workers 1,2,4         Explicit worker counts, each 1..16 (default: 1)
  --repeats N             Samples per worker count, 1..20 (default: 3)
  --timeout-seconds N     Child lifetime including fingerprint verification, 1..3600 (default: 120)
  --note TEXT             Hardware/storage/environment notes

Generates and verifies synthetic data before timing. Each sample runs in a fresh
process; OS cache is uncontrolled/likely warm. Existing reports are never replaced.
512 MiB conservative aggregate admission estimate; not an OS-enforced memory limit.
Ctrl-C cancels preparation or stops/reaps the active read-only measurement child.
";

struct Options {
    recipe: Recipe,
    scratch: PathBuf,
    output: PathBuf,
    workload: Workload,
    workers: Vec<usize>,
    repeats: usize,
    timeout: Duration,
    note: String,
}
fn parse(args: &[String]) -> Result<Options> {
    let mut result = Options {
        recipe: Recipe::default(),
        scratch: env::temp_dir(),
        output: PathBuf::new(),
        workload: Workload::Full,
        workers: vec![1],
        repeats: 3,
        timeout: Duration::from_secs(120),
        note: String::new(),
    };
    let mut seen = std::collections::HashSet::new();
    for pair in args.chunks(2) {
        ensure!(pair.len() == 2, "missing value for {}", pair[0]);
        let key = pair[0].as_str();
        let value = &pair[1];
        ensure!(seen.insert(key), "duplicate option {key}");
        match key {
            "--scratch" => result.scratch = value.into(),
            "--output" => result.output = value.into(),
            "--encoding" => {
                result.recipe.encoding = match value.as_str() {
                    "fits" => Encoding::Fits,
                    "xisf" => Encoding::Xisf,
                    "zlib" => Encoding::Zlib,
                    "zstd" => Encoding::Zstd,
                    _ => bail!("unknown encoding {value}"),
                }
            }
            "--pattern" => {
                result.recipe.pattern = match value.as_str() {
                    "noise" => Pattern::Noise,
                    "gradient" => Pattern::Gradient,
                    _ => bail!("unknown pattern {value}"),
                }
            }
            "--workload" => result.workload = workload(value)?,
            "--width" => result.recipe.width = value.parse()?,
            "--height" => result.recipe.height = value.parse()?,
            "--frames" => result.recipe.frames = value.parse()?,
            "--seed" => result.recipe.seed = value.parse()?,
            "--disk-mib" => {
                result.recipe.max_disk_bytes = value
                    .parse::<u64>()?
                    .checked_mul(1024 * 1024)
                    .context("disk quota overflow")?
            }
            "--workers" => {
                result.workers = value
                    .split(',')
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?
            }
            "--repeats" => result.repeats = value.parse()?,
            "--timeout-seconds" => result.timeout = Duration::from_secs(value.parse()?),
            "--note" => {
                ensure!(value.len() <= 8192, "environment note too long");
                result.note = value.clone();
            }
            _ => bail!("unknown option {key}"),
        }
    }
    result.recipe.validate()?;
    ensure!(
        !result.workers.is_empty() && result.workers.len() <= 16,
        "supply 1..16 worker counts"
    );
    for &workers in &result.workers {
        result.workload.check_budget(&result.recipe, workers)?;
    }
    let mut unique = result.workers.clone();
    unique.sort_unstable();
    unique.dedup();
    ensure!(
        unique.len() == result.workers.len(),
        "duplicate worker count"
    );
    ensure!(
        (1..=20).contains(&result.repeats),
        "repeats must be in 1..=20"
    );
    ensure!(
        (1..=3600).contains(&result.timeout.as_secs()),
        "timeout must be in 1..=3600 seconds"
    );
    ensure!(
        !result.output.as_os_str().is_empty(),
        "--output is required"
    );
    ensure!(
        !result.output.try_exists()?,
        "report already exists: {}",
        result.output.display()
    );
    Ok(result)
}
fn workload(value: &str) -> Result<Workload> {
    Ok(match value {
        "read" => Workload::Read,
        "structural" => Workload::Structural,
        "full" => Workload::Full,
        _ => bail!("unknown workload {value}"),
    })
}

#[derive(Serialize)]
struct Provenance {
    package_version: &'static str,
    build_profile: &'static str,
    rustc: &'static str,
    source_revision: &'static str,
    source_sha256: &'static str,
    os: &'static str,
    architecture: &'static str,
    available_parallelism: Option<usize>,
    environment_note: String,
    cache_state: &'static str,
    rss_source: &'static str,
    validator_limits: serde_json::Value,
}
#[derive(Serialize)]
struct Summary {
    workers: usize,
    repetitions: usize,
    min_wall_seconds: f64,
    median_wall_seconds: f64,
    max_wall_seconds: f64,
    median_stored_mib_per_second: f64,
    file_p50_seconds: f64,
    file_p95_seconds: f64,
    maximum_peak_rss_bytes: Option<u64>,
}
#[derive(Serialize)]
struct Report {
    schema_version: u32,
    provenance: Provenance,
    manifest: Manifest,
    finished_unix_seconds: u64,
    preparation_seconds: f64,
    timeout_seconds: u64,
    samples: Vec<Sample>,
    summaries: Vec<Summary>,
}
fn median(values: &[f64]) -> f64 {
    let n = values.len();
    if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    }
}
fn summarize(samples: &[Sample], workers: usize) -> Summary {
    let group: Vec<_> = samples.iter().filter(|s| s.workers == workers).collect();
    let mut wall: Vec<_> = group.iter().map(|s| s.wall_seconds).collect();
    wall.sort_by(f64::total_cmp);
    let mut rate: Vec<_> = group
        .iter()
        .map(|s| s.stored_bytes as f64 / (1024.0 * 1024.0) / s.wall_seconds)
        .collect();
    rate.sort_by(f64::total_cmp);
    let mut files: Vec<_> = group
        .iter()
        .flat_map(|s| s.files.iter().map(|f| f.seconds))
        .collect();
    files.sort_by(f64::total_cmp);
    Summary {
        workers,
        repetitions: group.len(),
        min_wall_seconds: wall[0],
        median_wall_seconds: median(&wall),
        max_wall_seconds: wall[wall.len() - 1],
        median_stored_mib_per_second: median(&rate),
        file_p50_seconds: median(&files),
        file_p95_seconds: files[(files.len() * 95).div_ceil(100) - 1],
        maximum_peak_rss_bytes: group.iter().filter_map(|s| s.peak_rss_bytes).max(),
    }
}
// Own every child until it has exited. This also handles error paths after spawn.
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn isolated(
    set: &FixtureSet,
    kind: Workload,
    workers: usize,
    timeout: Duration,
    cancel: &AtomicBool,
    number: usize,
) -> Result<Sample> {
    let path = set.directory().join(format!("sample-{number}.json"));
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let kind = match kind {
        Workload::Read => "read",
        Workload::Structural => "structural",
        Workload::Full => "full",
    };
    let start = Instant::now();
    let mut child = ChildGuard(
        Command::new(env::current_exe()?)
            .arg("sample")
            .arg(set.directory())
            .arg(kind)
            .arg(workers.to_string())
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(Stdio::inherit())
            .spawn()
            .context("start isolated sample")?,
    );
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "benchmark cancelled");
        ensure!(
            start.elapsed() < timeout,
            "sample timed out after {} seconds; child stopped",
            timeout.as_secs()
        );
        if let Some(status) = child.0.try_wait()? {
            ensure!(status.success(), "sample process failed: {status}");
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let mut bytes = Vec::new();
    File::open(&path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "oversized sample report");
    let sample: Sample = serde_json::from_slice(&bytes)?;
    fs::remove_file(path)?;
    Ok(sample)
}
fn run(options: Options, cancel: &AtomicBool) -> Result<()> {
    eprintln!(
        "Preparing {} synthetic {:?} frames outside timing...",
        options.recipe.frames, options.recipe.encoding
    );
    let start = Instant::now();
    let set = FixtureSet::generate(&options.scratch, options.recipe, cancel)?;
    let preparation_seconds = start.elapsed().as_secs_f64();
    let mut samples = Vec::new();
    for &workers in &options.workers {
        for repeat in 0..options.repeats {
            ensure!(!cancel.load(Ordering::Relaxed), "benchmark cancelled");
            eprintln!(
                "{:?}: {workers} worker(s), repetition {}/{}",
                options.workload,
                repeat + 1,
                options.repeats
            );
            samples.push(isolated(
                &set,
                options.workload,
                workers,
                options.timeout,
                cancel,
                samples.len(),
            )?);
        }
    }
    let summaries = options
        .workers
        .iter()
        .map(|&w| summarize(&samples, w))
        .collect();
    let limits = astro_io::validation::ValidationLimits::default();
    let report = Report {schema_version: 1, finished_unix_seconds: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(), preparation_seconds, timeout_seconds: options.timeout.as_secs(), manifest: set.manifest().clone(), samples, summaries, provenance: Provenance {
        package_version: env!("CARGO_PKG_VERSION"), build_profile: env!("BENCH_PROFILE"), rustc: env!("BENCH_RUSTC"), source_revision: env!("BENCH_REVISION"), source_sha256: env!("BENCH_SOURCE_SHA256"),
        os: env::consts::OS, architecture: env::consts::ARCH, available_parallelism: thread::available_parallelism().ok().map(usize::from), environment_note: options.note,
        cache_state: "uncontrolled / likely warm: generation and pre-sample fingerprint verification touch every file; fresh processes do not reset OS cache",
        rss_source: "process-lifetime getrusage(RUSAGE_SELF): macOS bytes, Linux KiB converted to bytes; null on other platforms; excludes parent generation but includes child setup",
        validator_limits: serde_json::json!({
            "max_header_bytes": limits.max_header_bytes(),
            "max_working_bytes_per_call": limits.max_working_bytes(),
            "max_structures": limits.max_structures(),
            "max_decoded_bytes": limits.max_decoded_bytes(),
            "runner_max_estimated_working_bytes": 512 * 1024 * 1024,
        }),
    }};
    ensure!(!cancel.load(Ordering::Relaxed), "benchmark cancelled");
    set.cleanup()?;
    let bytes = serde_json::to_vec_pretty(&report)?;
    write_report(&options.output, &bytes)?;
    eprintln!("Report written: {}", options.output.display());
    Ok(())
}

fn write_report(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("create report (existing files are preserved)")?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    match args[0].as_str() {
        "run" => run(parse(&args[1..])?, &cancel),
        // Internal read-only entry point. The parent owns directory cleanup.
        "sample" => {
            ensure!(args.len() == 4, "invalid internal sample arguments");
            let set = FixtureSet::open(Path::new(&args[1]), &cancel)?;
            let result = run_sample(&set, workload(&args[2])?, args[3].parse()?, &cancel, |_| {})?;
            serde_json::to_writer(std::io::stdout().lock(), &result)?;
            Ok(())
        }
        _ => bail!("expected 'run'; use --help"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_stops_and_reaps_child_before_scratch_cleanup() {
        let c = AtomicBool::new(false);
        let set = FixtureSet::generate(
            &env::temp_dir(),
            Recipe {
                width: 8,
                height: 8,
                frames: 1,
                ..Recipe::default()
            },
            &c,
        )
        .unwrap();
        // current_exe is this test binary; a zero deadline must cancel it before
        // interpreting its output. No wall-time threshold is used for this assertion.
        let error = isolated(&set, Workload::Full, 1, Duration::ZERO, &c, 0).unwrap_err();
        assert!(error.to_string().contains("timed out"));
        let path = set.directory().to_owned();
        set.cleanup().unwrap();
        assert!(!path.exists());
    }
}
