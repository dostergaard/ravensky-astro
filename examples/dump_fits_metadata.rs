use astro_io::fits::{backend::with_cfitsio, select_image_hdu};
use astro_metadata::fits_parser::extract_metadata_from_path;
use fitsio::{hdu::HduInfo, FitsFile};

#[path = "shared/metadata_dump.rs"]
mod metadata_dump;

fn main() {
    metadata_dump::run_metadata_dump_with_postscript(
        "FITS",
        "<fits_file_path>",
        "Raw FITS Header Cards",
        extract_metadata_from_path,
        |path| {
            with_cfitsio(|| {
                let mut file = FitsFile::open(path)?;
                let hdu = select_image_hdu(&mut file)?;
                if let HduInfo::ImageInfo { shape, image_type } = &hdu.info {
                    println!(
                        "\nSelected logical image: HDU {} ({}x{}, {:?})",
                        hdu.number, shape[1], shape[0], image_type
                    );
                }
                Ok(())
            })
        },
    );
}
