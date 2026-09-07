//! Reproducible synthetic workloads and measurements; no application tuning policy.
//!
//! Generate fixtures outside measurement, then call [`run_sample`] with explicitly
//! selected concurrency. The CLI isolates samples in fresh processes for peak RSS.
//! All workloads are read-only. Errors/cancellation never yield successful samples.
//!
//! For setup, CLI commands, report interpretation and a separate-project example,
//! see `docs/BenchmarkGuide.md` in the complete repository checkout. The crate's
//! `README.md` describes CLI options and measurement/resource contracts.
//!
//! ```no_run
//! use astro_bench::{FixtureSet, Recipe, Workload, run_sample};
//! use std::sync::atomic::AtomicBool;
//!
//! let cancel = AtomicBool::new(false);
//! let set = FixtureSet::generate(&std::env::temp_dir(), Recipe::default(), &cancel)?;
//! let sample = run_sample(&set, Workload::Full, 2, &cancel, |_| {})?;
//! assert_eq!(sample.completed_files, set.manifest().files.len());
//! set.cleanup()?;
//! # Ok::<(), anyhow::Error>(())
//! ```
#![warn(missing_docs)]

mod fixtures;
mod resources;
mod runner;

pub use fixtures::{Encoding, FileRecord, FixtureSet, Manifest, Pattern, Recipe};
pub use runner::{run_sample, FileMeasurement, Sample, Workload};

use anyhow::{ensure, Result};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "benchmark cancelled");
    Ok(())
}
