use crate::{checkpoint, resources::usage, Encoding, FixtureSet, Recipe};
use anyhow::{ensure, Context, Result};
use astro_io::validation::{validate_file, FileFormat, ValidationLevel, ValidationOptions};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

/// Separate operations; their throughput results must not be combined into one score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Workload {
    /// Read every stored byte with a fixed 64 KiB buffer per worker.
    Read,
    /// Validate declared container layout.
    Structural,
    /// Read all bytes, decode payloads and verify declared checksums.
    Full,
}
impl Workload {
    /// Check 1..=16 workers and a conservative 512 MiB aggregate admission estimate.
    ///
    /// Returns estimated bytes, not a hard RSS bound. Includes 16 MiB per worker
    /// for runtime/codec overhead; compressed full validation allows three image
    /// buffers plus expansion. This deliberately limits the initial measurement
    /// harness until shared validator reservations and native telemetry are available.
    pub fn check_budget(self, recipe: &Recipe, workers: usize) -> Result<u64> {
        let decoded = recipe.validate()?;
        ensure!((1..=16).contains(&workers), "workers must be in 1..=16");
        let buffers =
            if self == Self::Full && matches!(recipe.encoding, Encoding::Zlib | Encoding::Zstd) {
                decoded * 3 + decoded / 100 + 16384
            } else {
                65536
            };
        let estimate = (buffers + 16 * 1024 * 1024) * workers as u64;
        ensure!(
            estimate <= 512 * 1024 * 1024,
            "sample exceeds 512 MiB admission estimate; reduce workers or image dimensions"
        );
        Ok(estimate)
    }
}
/// One successfully completed file; durations include only its chosen operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeasurement {
    /// Input position in the manifest.
    pub index: usize,
    /// Time in the chosen operation, in seconds.
    pub seconds: f64,
    /// Managed physical reads, including validator rereads (not device I/O counters).
    pub read_bytes: u64,
}
/// Successful sample. Any failure or cancellation returns an error instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    /// Chosen operation.
    pub workload: Workload,
    /// Explicit fixed worker count (threads may outnumber frames).
    pub workers: usize,
    /// Files successfully completed; always equals the manifest count.
    pub completed_files: usize,
    /// Total physical input lengths, counted once per file.
    pub stored_bytes: u64,
    /// Declared image bytes, not a claim that structural/read mode decoded them.
    pub decoded_bytes: u64,
    /// Managed physical reads, possibly greater than stored bytes in full validation.
    pub read_bytes: u64,
    /// Elapsed seconds, including worker startup, callbacks and joins.
    pub wall_seconds: f64,
    /// User plus system CPU-time delta for this process; null when unavailable.
    pub cpu_seconds: Option<f64>,
    /// Process-lifetime high-water RSS, in bytes; macOS/Linux getrusage, null elsewhere.
    pub peak_rss_bytes: Option<u64>,
    /// Admission estimate used before starting threads; not a hard memory ceiling.
    pub estimated_working_bytes: u64,
    /// Per-file results, sorted in manifest order, not completion order.
    pub files: Vec<FileMeasurement>,
}

/// Run a read-only sample on prepared fixtures with bounded fixed concurrency.
///
/// The progress callback receives each completed file on the calling thread.
/// Keep it cheap: callback time is part of wall time. Cancellation is cooperative;
/// a blocked OS read cannot be interrupted. All threads are joined before returning
/// on success, worker failure or cancellation. No signal handlers are installed.
/// The input directory must remain exclusively under benchmark control.
///
/// RSS is process-lifetime: use the CLI's fresh child processes for comparable
/// samples, and do not label in-process application RSS as workload-only memory.
pub fn run_sample(
    set: &FixtureSet,
    workload: Workload,
    workers: usize,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&FileMeasurement),
) -> Result<Sample> {
    let estimated_working_bytes = workload.check_budget(&set.manifest().recipe, workers)?;
    checkpoint(cancel)?;
    let next = AtomicUsize::new(0);
    let stopped = AtomicBool::new(false);
    let before = usage();
    let start = Instant::now();
    let files = thread::scope(|scope| -> Result<Vec<FileMeasurement>> {
        let (tx, rx) = mpsc::sync_channel(workers * 2);
        let mut handles = Vec::with_capacity(workers);
        let mut failure = None;
        for _ in 0..workers {
            let tx = tx.clone();
            let next = &next;
            let stopped = &stopped;
            match thread::Builder::new()
                .name("astro-bench".into())
                .spawn_scoped(scope, move || {
                    while !stopped.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= set.manifest().files.len() {
                            break;
                        }
                        let result = measure_file(set, index, workload, cancel);
                        if result.is_err() {
                            stopped.store(true, Ordering::Relaxed);
                        }
                        if tx.send(result).is_err() {
                            break;
                        }
                    }
                }) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    stopped.store(true, Ordering::Relaxed);
                    failure = Some(anyhow::Error::from(error).context("start benchmark worker"));
                    break;
                }
            }
        }
        drop(tx);
        let mut files = Vec::with_capacity(set.manifest().files.len());
        // Drain even after failure: senders must finish before joining bounded-channel workers.
        for result in rx {
            match result {
                Ok(file) => {
                    progress(&file);
                    files.push(file);
                }
                Err(error) => {
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
            }
        }
        for handle in handles {
            if handle.join().is_err() {
                failure = Some(anyhow::anyhow!("benchmark worker panicked"));
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        checkpoint(cancel)?;
        ensure!(
            files.len() == set.manifest().files.len(),
            "incomplete sample"
        );
        files.sort_by_key(|f| f.index);
        Ok(files)
    })?;
    let wall_seconds = start.elapsed().as_secs_f64();
    let after = usage();
    Ok(Sample {
        workload,
        workers,
        completed_files: files.len(),
        stored_bytes: set.manifest().files.iter().map(|f| f.stored_bytes).sum(),
        decoded_bytes: set.manifest().files.iter().map(|f| f.decoded_bytes).sum(),
        read_bytes: files.iter().map(|f| f.read_bytes).sum(),
        wall_seconds,
        cpu_seconds: before
            .cpu_seconds
            .zip(after.cpu_seconds)
            .map(|(a, b)| (b - a).max(0.0)),
        peak_rss_bytes: after.peak_rss_bytes,
        estimated_working_bytes,
        files,
    })
}
fn measure_file(
    set: &FixtureSet,
    index: usize,
    workload: Workload,
    cancel: &AtomicBool,
) -> Result<FileMeasurement> {
    checkpoint(cancel)?;
    let entry = &set.manifest().files[index];
    let path = set.directory().join(&entry.name);
    let start = Instant::now();
    let read_bytes = if workload == Workload::Read {
        let mut file = File::open(&path).with_context(|| format!("open {}", path.display()))?;
        let mut bytes = [0; 65536];
        let mut read = 0;
        loop {
            checkpoint(cancel)?;
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            read += n as u64;
        }
        ensure!(read == entry.stored_bytes, "raw read length changed");
        read
    } else {
        let level = if workload == Workload::Full {
            ValidationLevel::Full
        } else {
            ValidationLevel::Structural
        };
        let report = validate_file(
            &path,
            &ValidationOptions::default().with_level(level),
            Some(cancel),
        )?;
        let encoding = set.manifest().recipe.encoding;
        let format = if encoding == Encoding::Fits {
            FileFormat::Fits
        } else {
            FileFormat::Xisf
        };
        let checksums = u64::from(encoding != Encoding::Fits);
        ensure!(
            report.format() == format
                && report.level() == level
                && report.image_count() == 1
                && report.stamp().size() == entry.stored_bytes
                && report.checksums().present() == checksums,
            "validation did not match fixture expectations"
        );
        if workload == Workload::Full {
            ensure!(
                report.checksums().unchecked() == 0 && report.undecoded_codecs().is_empty(),
                "incomplete full validation"
            );
        }
        report.bytes_read()
    };
    Ok(FileMeasurement {
        index,
        seconds: start.elapsed().as_secs_f64(),
        read_bytes,
    })
}
