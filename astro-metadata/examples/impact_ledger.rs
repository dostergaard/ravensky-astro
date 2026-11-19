//! Inspect the supported metadata implications of a proposed output format.
//! This read-only example does not grant loss approval or write an output.

use anyhow::{bail, ensure, Context, Result};
use astro_io::typed::{inventory, ContainerFormat, InventoryLimits};
use astro_metadata::fidelity::inspect;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let source = PathBuf::from(
        args.next()
            .context("Usage: impact_ledger SOURCE fits|xisf")?,
    );
    let format = args
        .next()
        .context("Usage: impact_ledger SOURCE fits|xisf")?;
    ensure!(
        args.next().is_none(),
        "Usage: impact_ledger SOURCE fits|xisf"
    );
    let destination = match format.to_str() {
        Some("fits") => ContainerFormat::Fits,
        Some("xisf") => ContainerFormat::Xisf,
        _ => bail!("destination must be fits or xisf"),
    };

    let source_inventory = inventory(&source, InventoryLimits::default())?;
    let ledger = inspect(&source_inventory, destination)?;
    println!("Eligibility: {:?}", ledger.eligibility);
    for item in &ledger.items {
        println!(
            "{}: {:?} — {}",
            item.identifier, item.state, item.description
        );
    }
    Ok(())
}
