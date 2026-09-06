use astro_io::fits::backend::with_cfitsio;
use astro_io::fits::{load_fits, read_all_header_cards};
use astro_io::validation::{validate_file, ValidationLevel, ValidationOptions};
use astro_metadata::fits_parser::{extract_metadata, extract_metadata_from_path};
use fitsio::images::{ImageDescription, ImageType};
use fitsio::FitsFile;
use std::sync::Barrier;

#[test]
fn concurrent_loaders_metadata_and_validation_use_independent_handles() {
    let path = std::env::temp_dir().join(format!(
        "astro-native-concurrency-{}-{}.fits",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let compressed = path.with_extension("fz");
    let pixels = [1.0f32, 2.0, 3.0, 4.0];
    with_cfitsio(|| {
        let mut file = FitsFile::create(&path)
            .with_custom_primary(&ImageDescription {
                data_type: ImageType::Float,
                dimensions: &[2, 2],
            })
            .open()
            .unwrap();
        let hdu = file.primary_hdu().unwrap();
        hdu.write_image(&mut file, &pixels).unwrap();
        hdu.write_key(&mut file, "OBJECT", "M42").unwrap();
    });
    with_cfitsio(|| {
        let mut input = FitsFile::open(&path).unwrap();
        let mut output = FitsFile::create(&compressed).open().unwrap();
        let mut status = 0;
        // SAFETY: both handles are live and separately owned throughout the call;
        // status is writable, and native admission covers open through drop.
        unsafe {
            fitsio::sys::fits_set_compression_type(
                output.as_raw(),
                fitsio::sys::GZIP_1 as i32,
                &mut status,
            );
            fitsio::sys::fits_img_compress(input.as_raw(), output.as_raw(), &mut status);
        }
        assert_eq!(status, 0);
    });
    let barrier = Barrier::new(4);
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let path = &path;
            let compressed = &compressed;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..20 {
                    match worker {
                        0 => assert_eq!(load_fits(path).unwrap(), (pixels.to_vec(), 2, 2)),
                        1 => {
                            let metadata = extract_metadata_from_path(path).unwrap();
                            assert_eq!(metadata.raw_headers["OBJECT"], "M42");
                        }
                        2 => with_cfitsio(|| {
                            let mut file = FitsFile::open(path).unwrap();
                            let metadata = extract_metadata(&mut file).unwrap();
                            assert_eq!(metadata.raw_headers["OBJECT"], "M42");
                            assert!(read_all_header_cards(&mut file)
                                .unwrap()
                                .iter()
                                .any(|card| card.keyword == "OBJECT"));
                        }),
                        _ => {
                            let report = with_cfitsio(|| {
                                validate_file(
                                    compressed,
                                    &ValidationOptions::default().with_level(ValidationLevel::Full),
                                    None,
                                )
                            })
                            .unwrap();
                            assert_eq!(report.image_count(), 1);
                        }
                    }
                }
            });
        }
    });
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(compressed).unwrap();
}
