//! Read-only capture measurement, separate from synthetic recipe generation.
//! Run through a fresh process for each sample; see the crate README.
use anyhow::{ensure, Context, Result};
use astro_io::validation::{
    validate_file_with_budget, MemoryBudget, ValidationLevel, ValidationLimits, ValidationOptions,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, Metadata},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Instant, SystemTime},
};

// Reuse the same isolated platform telemetry as the synthetic runner.
#[path = "../src/resources.rs"]
mod resources;

#[derive(PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: Option<SystemTime>,
    identity: Option<(u64, u64)>,
}
fn stamp(meta: &Metadata) -> Stamp {
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        Some((meta.dev(), meta.ino()))
    };
    #[cfg(not(unix))]
    let identity = None;
    Stamp {
        size: meta.len(),
        modified: meta.modified().ok(),
        identity,
    }
}
fn fingerprint(path: &Path, cancel: &AtomicBool) -> Result<String> {
    let mut file = File::open(path)?;
    let before = stamp(&file.metadata()?);
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut size = 0;
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "capture probe cancelled");
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        ensure!(size <= before.size, "capture grew during hashing");
        hash.update(&buffer[..count]);
    }
    ensure!(
        size == before.size
            && stamp(&file.metadata()?) == before
            && stamp(&fs::metadata(path)?) == before,
        "capture changed during hashing"
    );
    Ok(format!("{:x}", hash.finalize()))
}

fn measure(
    paths: &[PathBuf],
    level: ValidationLevel,
    workers: usize,
    passes: usize,
    cancel: &AtomicBool,
) -> Result<Value> {
    ensure!((1..=8).contains(&workers), "workers must be in 1..8");
    ensure!((1..=256).contains(&passes), "passes must be in 1..256");
    ensure!(
        !paths.is_empty() && paths.len() <= 256,
        "require 1..256 input files"
    );
    let count = paths.len() * passes;
    ensure!(count <= 4096, "at most 4096 operations per process");
    let mut unique = HashSet::new();
    let mut sizes = Vec::new();
    let mut stamps = Vec::new();
    let mut total = 0_u64;
    for path in paths {
        let meta = fs::symlink_metadata(path)?;
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "require regular non-symlink inputs"
        );
        ensure!(
            unique.insert(path.canonicalize()?),
            "duplicate capture path"
        );
        total = total
            .checked_add(meta.len())
            .context("input size overflow")?;
        ensure!(
            total <= 8 * 1024 * 1024 * 1024,
            "input set exceeds 8 GiB stored bytes"
        );
        sizes.push(meta.len());
        stamps.push(stamp(&meta));
    }
    let preparation = Instant::now();
    let hashes: Vec<_> = paths
        .iter()
        .map(|p| fingerprint(p, cancel))
        .collect::<Result<_>>()?;
    let preparation_seconds = preparation.elapsed().as_secs_f64();
    let budget = MemoryBudget::new(512 * 1024 * 1024)?;
    let per_call = (budget.capacity_bytes() / workers as u64).min(256 * 1024 * 1024);
    let options = ValidationOptions::default()
        .with_level(level)
        .with_limits(ValidationLimits::default().with_max_working_bytes(per_call)?);
    let next = AtomicUsize::new(0);
    let before = resources::usage();
    let start = Instant::now();
    let mut operations = thread::scope(|scope| -> Result<Vec<Value>> {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let next = &next;
            let options = &options;
            let budget = &budget;
            handles.push(thread::Builder::new().name("capture-probe".into()).spawn_scoped(scope, move || {
                let mut results = Vec::new();
                while !cancel.load(Ordering::Relaxed) {
                    let job = next.fetch_add(1, Ordering::Relaxed);
                    if job >= count { break; }
                    let index = job % paths.len();
                    let start = Instant::now();
                    let result = validate_file_with_budget(&paths[index], options, Some(cancel), budget);
                    let seconds = start.elapsed().as_secs_f64();
                    let mut value = json!({"index": index, "pass": job / paths.len(), "seconds": seconds});
                    match result {
                        Ok(report) => {
                            value["success"] = true.into();
                            value["format"] = format!("{:?}", report.format()).into();
                            value["images"] = report.image_count().into();
                            value["read_bytes"] = report.bytes_read().into();
                            value["checksums_present"] = report.checksums().present().into();
                            value["checksums_verified"] = report.checksums().verified().into();
                            value["undecoded_codec_count"] = report.undecoded_codecs().len().into();
                        }
                        Err(error) => {
                            value["success"] = false.into();
                            // No paths or capture metadata in retained reports.
                            value["error_kind"] = format!("{:?}", error.kind()).into();
                        }
                    }
                    results.push(value);
                }
                results
            })?);
        }
        let mut operations = Vec::new();
        for handle in handles {
            operations.extend(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("capture worker panicked"))?,
            );
        }
        Ok(operations)
    })?;
    let wall_seconds = start.elapsed().as_secs_f64();
    let after = resources::usage();
    ensure!(budget.used_bytes() == 0, "capture reservations leaked");
    ensure!(!cancel.load(Ordering::Relaxed), "capture probe cancelled");
    operations.sort_by_key(|v| (v["pass"].as_u64(), v["index"].as_u64()));
    let mut inputs = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        ensure!(
            stamp(&fs::metadata(path)?) == stamps[index]
                && fingerprint(path, cancel)? == hashes[index],
            "capture changed during measurement"
        );
        inputs.push(json!({"index": index, "stored_bytes": sizes[index], "sha256": hashes[index]}));
    }
    let complete = operations.len() == count && operations.iter().all(|o| o["success"] == true);
    Ok(json!({
        "schema_version": 1, "kind": "capture_probe", "complete": complete,
        "sources_unchanged": true, "inputs": inputs, "operations": operations,
        "workers": workers, "passes": passes, "level": format!("{level:?}"),
        "wall_seconds": wall_seconds, "preparation_seconds": preparation_seconds,
        "cpu_seconds": before.cpu_seconds.zip(after.cpu_seconds).map(|(a,b)| (b-a).max(0.0)),
        "peak_rss_bytes": after.peak_rss_bytes, "peak_reserved_bytes": budget.peak_bytes(),
        "reserved_bytes_after": budget.used_bytes(), "per_call_working_bytes": per_call,
        "shared_memory_bytes": budget.capacity_bytes(),
        "source_revision": env!("BENCH_REVISION"), "source_sha256": env!("BENCH_SOURCE_SHA256"),
        "probe_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("capture_probe.rs"))),
        "build_profile": env!("BENCH_PROFILE"), "rustc": env!("BENCH_RUSTC"),
        "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "cache_state": "uncontrolled / likely warm: fingerprints touch inputs before timing",
        "rss_source": "process-lifetime getrusage: macOS bytes, Linux KiB converted; null elsewhere"
    }))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() >= 4,
        "usage: capture_probe structural|full WORKERS PASSES FILE..."
    );
    let level = match args[0].as_str() {
        "structural" => ValidationLevel::Structural,
        "full" => ValidationLevel::Full,
        _ => anyhow::bail!("expected structural or full"),
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    let paths: Vec<_> = args[3..].iter().map(PathBuf::from).collect();
    let result = measure(&paths, level, args[1].parse()?, args[2].parse()?, &cancel)?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &result)?;
    if result["complete"] != true {
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use astro_bench::{Encoding, FixtureSet, Recipe};

    #[test]
    fn mixed_captures_are_counted_and_preserved_across_passes() {
        let cancel = AtomicBool::new(false);
        let mut sets = Vec::new();
        let mut paths = Vec::new();
        for encoding in [Encoding::Fits, Encoding::Zlib] {
            let set = FixtureSet::generate(
                &std::env::temp_dir(),
                Recipe {
                    width: 8,
                    height: 8,
                    frames: 1,
                    encoding,
                    ..Recipe::default()
                },
                &cancel,
            )
            .unwrap();
            paths.push(set.directory().join(&set.manifest().files[0].name));
            sets.push(set);
        }
        let result = measure(&paths, ValidationLevel::Full, 2, 3, &cancel).unwrap();
        assert_eq!(result["complete"], true);
        assert_eq!(result["operations"].as_array().unwrap().len(), 6);
        assert_eq!(result["sources_unchanged"], true);
        assert_eq!(result["reserved_bytes_after"], 0);
        for set in sets {
            FixtureSet::open(set.directory(), &cancel).unwrap();
        }
    }

    #[test]
    fn invalid_capture_is_diagnostic_not_successful_throughput() {
        let cancel = AtomicBool::new(false);
        let set = FixtureSet::generate(
            &std::env::temp_dir(),
            Recipe {
                width: 8,
                height: 8,
                frames: 1,
                ..Recipe::default()
            },
            &cancel,
        )
        .unwrap();
        let path = set.directory().join("invalid.fits");
        std::fs::write(&path, b"invalid FITS").unwrap();
        let result = measure(&[path], ValidationLevel::Full, 1, 1, &cancel).unwrap();
        assert_eq!(result["complete"], false);
        assert!(result["operations"][0]["error_kind"].is_string());
        assert_eq!(result["reserved_bytes_after"], 0);
    }

    #[test]
    fn reject_invalid_work_bounds_before_file_access() {
        for (workers, passes) in [(0, 1), (9, 1), (1, 0), (1, 257)] {
            assert!(measure(
                &[PathBuf::from("absent")],
                ValidationLevel::Full,
                workers,
                passes,
                &AtomicBool::new(false)
            )
            .is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn reject_symlinks_and_duplicate_capture_paths() {
        let cancel = AtomicBool::new(false);
        let set = FixtureSet::generate(
            &std::env::temp_dir(),
            Recipe {
                width: 8,
                height: 8,
                frames: 1,
                ..Recipe::default()
            },
            &cancel,
        )
        .unwrap();
        let path = set.directory().join(&set.manifest().files[0].name);
        let link = set.directory().join("link.fits");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(measure(&[link], ValidationLevel::Full, 1, 1, &cancel).is_err());
        assert!(measure(&[path.clone(), path], ValidationLevel::Full, 1, 1, &cancel).is_err());
    }
}
