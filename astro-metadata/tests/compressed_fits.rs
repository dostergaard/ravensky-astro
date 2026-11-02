use astro_io::fits::{backend::with_cfitsio, load_fits};
use astro_io::validation::{validate_file, ValidationLevel, ValidationOptions};
use astro_metadata::fits_parser::extract_metadata_from_path;
use fitsio::{
    images::{ImageDescription, ImageType},
    FitsFile,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files(Vec<PathBuf>);

impl Files {
    fn path(&mut self, suffix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "astro-compressed-metadata-{}-{}-{suffix}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        self.0.push(path.clone());
        path
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

#[test]
fn gzip2_image_extension_supplies_logical_dimensions_and_observation() {
    with_cfitsio(|| {
        let mut files = Files(Vec::new());
        let source = files.path("source.fits");
        let compressed = files.path("compressed.fits.fz");
        let pixels = [1i16, 2, 3, 4];
        {
            let mut file = FitsFile::create(&source)
                .with_custom_primary(&ImageDescription {
                    data_type: ImageType::Short,
                    dimensions: &[2, 2],
                })
                .open()
                .unwrap();
            let hdu = file.primary_hdu().unwrap();
            hdu.write_image(&mut file, &pixels).unwrap();
            hdu.write_key(&mut file, "OBJECT", "M42").unwrap();
            hdu.write_key(&mut file, "IMAGETYP", "LIGHT").unwrap();
            hdu.write_key(&mut file, "FILTER", "Ha").unwrap();
            hdu.write_key(&mut file, "EXPTIME", 120.0f32).unwrap();
            hdu.write_key(&mut file, "INSTRUME", "Artemis-M PRO")
                .unwrap();
        }
        {
            let mut input = FitsFile::open(&source).unwrap();
            let mut output = FitsFile::create(&compressed).open().unwrap();
            let mut status = 0;
            // SAFETY: both handles and writable status live through the native call.
            unsafe {
                fitsio::sys::fits_set_compression_type(
                    output.as_raw(),
                    fitsio::sys::GZIP_2 as i32,
                    &mut status,
                );
                fitsio::sys::fits_img_compress(input.as_raw(), output.as_raw(), &mut status);
            }
            assert_eq!(status, 0);
        }

        let mut file = FitsFile::open(&compressed).unwrap();
        assert_eq!(file.num_hdus().unwrap(), 2);
        assert!(matches!(
            file.hdu(1).unwrap().info,
            fitsio::hdu::HduInfo::ImageInfo { .. }
        ));
        drop(file);

        let metadata = extract_metadata_from_path(&compressed).unwrap();
        assert_eq!((metadata.detector.width, metadata.detector.height), (2, 2));
        assert_eq!(metadata.exposure.object_name.as_deref(), Some("M42"));
        assert_eq!(metadata.exposure.frame_type.as_deref(), Some("LIGHT"));
        assert_eq!(metadata.exposure.exposure_time, Some(120.0));
        assert_eq!(metadata.filter.name.as_deref(), Some("Ha"));
        assert_eq!(
            metadata.detector.camera_name.as_deref(),
            Some("Artemis-M PRO")
        );
        assert!(metadata
            .raw_header_cards
            .iter()
            .any(|card| { card.hdu_index == 1 && card.keyword == "ZIMAGE" }));
        assert_eq!(
            load_fits(&compressed).unwrap(),
            (vec![1.0, 2.0, 3.0, 4.0], 2, 2)
        );
    });
}

#[test]
fn first_usable_image_wins_and_its_keywords_override_primary() {
    with_cfitsio(|| {
        let mut files = Files(Vec::new());
        let path = files.path("multiple.fits");
        {
            let mut file = FitsFile::create(&path).open().unwrap();
            let primary = file.primary_hdu().unwrap();
            primary.write_key(&mut file, "OBJECT", "Primary").unwrap();
            primary.write_key(&mut file, "FILTER", "R").unwrap();
            let table = file.create_table("AUX".to_owned(), &[]).unwrap();
            table.write_key(&mut file, "OBJECT", "Table").unwrap();
            let description = ImageDescription {
                data_type: ImageType::Short,
                dimensions: &[3, 2],
            };
            let first = file.create_image("SCI".to_owned(), &description).unwrap();
            first.write_key(&mut file, "OBJECT", "Selected").unwrap();
            first.write_key(&mut file, "EXPTIME", 30.0f32).unwrap();
            let second = file.create_image("OTHER".to_owned(), &description).unwrap();
            second.write_key(&mut file, "OBJECT", "Other").unwrap();
        }
        let metadata = extract_metadata_from_path(&path).unwrap();
        assert_eq!((metadata.detector.width, metadata.detector.height), (2, 3));
        assert_eq!(metadata.exposure.object_name.as_deref(), Some("Selected"));
        assert_eq!(metadata.filter.name.as_deref(), Some("R"));
        assert_eq!(metadata.exposure.exposure_time, Some(30.0));
        assert!(metadata
            .raw_header_cards
            .iter()
            .any(|card| { card.hdu_index == 3 && card.keyword == "OBJECT" }));
        assert_eq!(
            metadata.raw_headers.get("OBJECT").map(String::as_str),
            Some("Other")
        );
        assert_eq!(load_fits(&path).unwrap().1, 2);
    });
}

#[test]
fn ordinary_primary_image_preserves_metadata_and_missing_fields() {
    with_cfitsio(|| {
        let mut files = Files(Vec::new());
        let path = files.path("ordinary.fits");
        {
            let mut file = FitsFile::create(&path)
                .with_custom_primary(&ImageDescription {
                    data_type: ImageType::Short,
                    dimensions: &[4, 3],
                })
                .open()
                .unwrap();
            file.primary_hdu()
                .unwrap()
                .write_key(&mut file, "OBJECT", "M31")
                .unwrap();
        }
        let metadata = extract_metadata_from_path(&path).unwrap();
        assert_eq!((metadata.detector.width, metadata.detector.height), (3, 4));
        assert_eq!(metadata.exposure.object_name.as_deref(), Some("M31"));
        assert_eq!(metadata.exposure.exposure_time, None);
        assert_eq!(metadata.filter.name, None);
        assert!(metadata
            .raw_header_cards
            .iter()
            .all(|card| card.hdu_index == 0));
    });
}

#[test]
fn empty_primary_without_image_is_not_reported_as_usable_metadata() {
    with_cfitsio(|| {
        let mut files = Files(Vec::new());
        let path = files.path("empty.fits");
        FitsFile::create(&path).open().unwrap();
        assert!(extract_metadata_from_path(&path).is_err());
    });
}

#[test]
fn supplied_nina_gzip2_headers_and_validation_match_canonical_metadata() {
    let Some(directory) = std::env::var_os("ASTRO_NINA_FITS_FIXTURES") else {
        return;
    };
    let mut observations = Vec::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let metadata = extract_metadata_from_path(&path).unwrap();
        assert_eq!(
            (metadata.detector.width, metadata.detector.height),
            (8288, 5648)
        );
        assert!(metadata.raw_header_cards.iter().any(|card| {
            card.hdu_index == 0 && card.keyword == "NAXIS" && card.value.as_deref() == Some("0")
        }));
        assert!(metadata.raw_header_cards.iter().any(|card| {
            card.hdu_index == 1 && card.keyword == "ZIMAGE" && card.value.as_deref() == Some("T")
        }));
        assert_eq!(
            metadata.detector.camera_name.as_deref(),
            Some("Artemis-M PRO")
        );
        assert_eq!(metadata.detector.gain, Some(120.0));
        assert_eq!(metadata.detector.offset, Some(5));
        assert_eq!(metadata.detector.temperature, Some(-10.0));
        assert_eq!(metadata.exposure.frame_type.as_deref(), Some("LIGHT"));
        let object = metadata.exposure.object_name.unwrap();
        let filter = metadata.filter.name.unwrap();
        let exposure = metadata.exposure.exposure_time.unwrap();
        observations.push((object, filter, exposure as i32));
        for level in [ValidationLevel::Structural, ValidationLevel::Full] {
            let report =
                validate_file(&path, &ValidationOptions::default().with_level(level), None)
                    .unwrap();
            assert_eq!(report.image_count(), 1);
        }
    }
    observations.sort();
    assert_eq!(
        observations,
        [
            ("Barnard_7".to_owned(), "R".to_owned(), 120),
            ("OrionsBelt".to_owned(), "Ha".to_owned(), 300),
            ("Sh2-119".to_owned(), "B".to_owned(), 120),
        ]
    );
}
