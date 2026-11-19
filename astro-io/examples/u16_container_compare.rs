//! Compare exact logical UInt16 image content in two already existing files.
//! This does not establish metadata equivalence or authorize a conversion.

use anyhow::{ensure, Context, Result};
use astro_io::typed::compare::compare_u16_containers;
use astro_io::typed::{inventory, InventoryLimits, PixelReadLimits};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let source = PathBuf::from(
        args.next()
            .context("Usage: u16_container_compare SOURCE OUTPUT")?,
    );
    let output = PathBuf::from(
        args.next()
            .context("Usage: u16_container_compare SOURCE OUTPUT")?,
    );
    ensure!(
        args.next().is_none(),
        "Usage: u16_container_compare SOURCE OUTPUT"
    );

    let source_inventory = inventory(&source, InventoryLimits::default())?;
    let output_inventory = inventory(&output, InventoryLimits::default())?;
    match compare_u16_containers(
        &source,
        &source_inventory,
        &output,
        &output_inventory,
        PixelReadLimits::default(),
    )? {
        None => println!("All supported UInt16 images match exactly"),
        Some(mismatch) => println!("Image mismatch: {mismatch:?}"),
    }
    Ok(())
}
