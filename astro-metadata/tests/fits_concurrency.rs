use astro_io::fits::backend::with_cfitsio;
use astro_io::fits::{load_fits, read_all_header_cards};
use astro_io::typed::{inventory, read_u16_chunk, InventoryLimits, PixelReadLimits};
use astro_io::validation::{validate_file, ValidationLevel, ValidationOptions};
use astro_metadata::fits_parser::{extract_metadata, extract_metadata_from_path};
use fitsio::images::{ImageDescription, ImageType};
use fitsio::FitsFile;
use std::path::{Path, PathBuf};
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

struct HcompressScratch(PathBuf);

impl HcompressScratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "astro-hcompress-concurrency-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for HcompressScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn hcompress_fixture(directory: &Path, name: &str, salt: u32) -> (PathBuf, Vec<u16>) {
    const WIDTH: usize = 64;
    const HEIGHT: usize = 64;
    let source = directory.join(format!("{name}-source.fits"));
    let compressed = directory.join(format!("{name}.fits"));
    let pixels: Vec<u16> = (0..WIDTH * HEIGHT)
        .map(|index| ((index as u32 * 7919 + salt) % 65536) as u16)
        .collect();
    with_cfitsio(|| {
        let mut input = FitsFile::create(&source)
            .with_custom_primary(&ImageDescription {
                data_type: ImageType::UnsignedShort,
                dimensions: &[WIDTH, HEIGHT],
            })
            .open()
            .unwrap();
        let hdu = input.primary_hdu().unwrap();
        hdu.write_image(&mut input, &pixels).unwrap();
        hdu.write_key(&mut input, "OBJECT", name).unwrap();
        drop(input);

        let mut input = FitsFile::open(&source).unwrap();
        let mut output = FitsFile::create(&compressed).open().unwrap();
        let mut tiles = [WIDTH as std::os::raw::c_long, 16];
        let mut status = 0;
        // SAFETY: both owned handles remain live through compression, and
        // the two-dimensional tile array and status storage are writable.
        unsafe {
            fitsio::sys::fits_set_compression_type(
                output.as_raw(),
                fitsio::sys::HCOMPRESS_1 as i32,
                &mut status,
            );
            fitsio::sys::fits_set_tile_dim(output.as_raw(), 2, tiles.as_mut_ptr(), &mut status);
            fitsio::sys::fits_img_compress(input.as_raw(), output.as_raw(), &mut status);
        }
        assert_eq!(status, 0, "create {name}");
    });
    (compressed, pixels)
}

fn checksum(pixels: &[u16]) -> u64 {
    pixels.iter().fold(0xcbf29ce484222325, |digest, sample| {
        (digest ^ u64::from(*sample)).wrapping_mul(0x100000001b3)
    })
}

#[test]
fn concurrent_hcompress_readers_keep_independent_files_and_handles_exact() {
    let scratch = HcompressScratch::new();
    let fixtures = [
        hcompress_fixture(&scratch.0, "M42", 17),
        hcompress_fixture(&scratch.0, "M31", 1009),
    ];
    let expected_checksums = [checksum(&fixtures[0].1), checksum(&fixtures[1].1)];
    assert_eq!(expected_checksums, [0xe36fa1e56e700325, 0x3b9125d5e279c325]);
    let source_bytes = [
        std::fs::read(&fixtures[0].0).unwrap(),
        std::fs::read(&fixtures[1].0).unwrap(),
    ];
    let validation = ValidationOptions::default().with_level(ValidationLevel::Full);
    let barrier = Barrier::new(4);
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let fixtures = &fixtures;
            let expected_checksums = &expected_checksums;
            let validation = &validation;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for repeat in 0..24 {
                    let index = (worker + repeat) % fixtures.len();
                    let (path, expected) = &fixtures[index];
                    let found = inventory(path, InventoryLimits::default()).unwrap();
                    assert_eq!(found.images[0].compression.as_deref(), Some("HCOMPRESS_1"));

                    let mut exact = vec![0u16; expected.len()];
                    read_u16_chunk(path, &found, 0, 0, &mut exact, PixelReadLimits::default())
                        .unwrap();
                    assert_eq!(exact, *expected);
                    assert_eq!(checksum(&exact), expected_checksums[index]);

                    let (float_pixels, width, height) = load_fits(path).unwrap();
                    assert_eq!((width, height), (64, 64));
                    assert_eq!(
                        float_pixels,
                        expected
                            .iter()
                            .map(|&sample| f32::from(sample))
                            .collect::<Vec<_>>()
                    );

                    let direct: Vec<u16> = with_cfitsio(|| {
                        let mut file = FitsFile::open(path).unwrap();
                        file.hdu(1).unwrap().read_image(&mut file).unwrap()
                    });
                    assert_eq!(checksum(&direct), expected_checksums[index]);
                    assert_eq!(direct, *expected);

                    let metadata = extract_metadata_from_path(path).unwrap();
                    assert_eq!(
                        metadata.exposure.object_name.as_deref(),
                        Some(if index == 0 { "M42" } else { "M31" })
                    );
                    assert_eq!(
                        validate_file(path, validation, None).unwrap().image_count(),
                        1
                    );
                }
            });
        }
    });
    for (fixture, original) in fixtures.iter().zip(source_bytes) {
        assert_eq!(std::fs::read(&fixture.0).unwrap(), original);
    }
}
