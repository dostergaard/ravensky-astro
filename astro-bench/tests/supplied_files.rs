use astro_io::validation::{
    validate_file_with_budget, MemoryBudget, ValidationErrorKind, ValidationLevel,
    ValidationOptions,
};
use std::path::Path;

#[test]
fn supplied_valid_fits_and_false_checksum_follow_level_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_data");
    let budget = MemoryBudget::new(16 * 1024 * 1024).unwrap();
    for name in [
        "blank.fits",
        "checksum.fits",
        "scale.fits",
        "tdim.fits",
        "test0.fits",
        "variable_length_table.fits",
        "checksum_false.fits",
    ] {
        for level in [ValidationLevel::Structural, ValidationLevel::Full] {
            let result = validate_file_with_budget(
                &root.join(name),
                &ValidationOptions::default().with_level(level),
                None,
                &budget,
            );
            if name == "checksum_false.fits" && level == ValidationLevel::Full {
                assert_eq!(
                    result.unwrap_err().kind(),
                    ValidationErrorKind::IntegrityMismatch
                );
            } else {
                assert!(result.is_ok(), "{name}, {level:?}: {result:?}");
            }
            assert_eq!(budget.used_bytes(), 0);
        }
    }
}

#[test]
fn supplied_xisf_missing_thumbnails_are_incomplete_at_both_levels() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_data");
    let budget = MemoryBudget::new(16 * 1024 * 1024).unwrap();
    for name in ["test.xisf", "2ch.xisf"] {
        for level in [ValidationLevel::Structural, ValidationLevel::Full] {
            let error = validate_file_with_budget(
                &root.join(name),
                &ValidationOptions::default().with_level(level),
                None,
                &budget,
            )
            .unwrap_err();
            assert_eq!(error.kind(), ValidationErrorKind::Incomplete);
            assert_eq!(budget.used_bytes(), 0);
        }
    }
}

#[test]
fn supplied_rice_files_validate_with_shared_budgets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_data");
    let budget = MemoryBudget::new(16 * 1024 * 1024).unwrap();
    for name in [
        "compressed_image.fits",
        "compressed_float_bzero.fits",
        "double_ext.fits",
    ] {
        validate_file_with_budget(
            &root.join(name),
            &ValidationOptions::default(),
            None,
            &budget,
        )
        .unwrap();
        validate_file_with_budget(
            &root.join(name),
            &ValidationOptions::default().with_level(ValidationLevel::Full),
            None,
            &budget,
        )
        .unwrap();
        assert_eq!(budget.used_bytes(), 0);
    }
}
