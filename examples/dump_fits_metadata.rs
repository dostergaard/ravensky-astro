use astro_metadata::fits_parser::extract_metadata_from_path;

#[path = "shared/metadata_dump.rs"]
mod metadata_dump;

fn main() {
    metadata_dump::run_metadata_dump_with_postscript(
        "FITS",
        "<fits_file_path>",
        "Raw FITS Header Cards",
        extract_metadata_from_path,
        |_| Ok(()),
    );
}
