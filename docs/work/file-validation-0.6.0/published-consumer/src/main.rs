use astro_io::validation::{FileFormat, MemoryBudget, ValidationLevel, ValidationOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new("published-smoke.fits");
    let mut bytes = Vec::new();
    for card in [
        "SIMPLE  =                    T",
        "BITPIX  =                   16",
        "NAXIS   =                    2",
        "NAXIS1  =                    2",
        "NAXIS2  =                    2",
        "OBJECT  = 'Release smoke'",
        "END",
    ] {
        bytes.extend_from_slice(format!("{card:<80}").as_bytes());
    }
    bytes.resize(2880, b' ');
    for pixel in [1_i16, 2, 3, 4] {
        bytes.extend_from_slice(&pixel.to_be_bytes());
    }
    bytes.resize(5760, 0);
    // Never overwrite a pre-existing file when reproducing this smoke test.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    std::io::Write::write_all(&mut file, &bytes)?;
    drop(file);
    let budget = MemoryBudget::new(64 * 1024 * 1024)?;
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let report =
        ravensky_astro::io::validation::validate_file_with_budget(path, &options, None, &budget)?;
    assert_eq!(report.format(), FileFormat::Fits);
    assert_eq!(report.level(), ValidationLevel::Full);
    assert_eq!(report.image_count(), 1);
    assert_eq!(budget.used_bytes(), 0);
    let _metadata = astro_metadata::fits_parser::extract_metadata_from_path(path)?;
    let (pixels, width, height) = astro_io::fits::load_fits(path)?;
    assert_eq!((width, height), (2, 2));
    assert_eq!(pixels, vec![1.0, 2.0, 3.0, 4.0]);
    let _metrics_api = astro_metrics::quality_metrics::calculate_quality_scores;
    assert_eq!(std::fs::read(path)?, bytes);
    std::fs::remove_file(path)?;
    println!(
        "Published 0.6.0 chain passed: full validation, shared budget released, metadata, pixel loading, facade, source preservation"
    );
    Ok(())
}
