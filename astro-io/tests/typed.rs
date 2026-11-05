use astro_io::typed::compare::{compare_u16_containers, ImageMismatch};
use astro_io::typed::write::{
    fits_output_bound_u16, write_fits_u16, write_xisf_u16, xisf_output_bound_u16, EncodedSizeBound,
    FitsCodec, U16ImageSpec, XisfCodec,
};
use astro_io::typed::{
    inventory, read_u16_chunk, ContainerFormat, InventoryLimits, PixelReadLimits,
};
use fitsio::{
    images::{ImageDescription, ImageType},
    FitsFile,
};
use std::io::Write;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn path(suffix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "astro-typed-{}-{}.{suffix}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn write_xisf_fixture(xml: &str, payload: &[u8]) -> Fixture {
    let fixture = Fixture::path("xisf");
    let mut bytes = b"XISF0100".to_vec();
    bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(xml.as_bytes());
    bytes.resize(4096, 0);
    bytes.extend_from_slice(payload);
    fs::write(&fixture.0, bytes).unwrap();
    fixture
}

fn encode(codec: &str, bytes: &[u8]) -> Vec<u8> {
    match codec {
        "zstd" => zstd::stream::encode_all(bytes, 1).unwrap(),
        "zlib" => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        }
        "lz4" => lz4_flex::block::compress(bytes),
        _ => unreachable!(),
    }
}

fn shuffle_u16(bytes: &[u8]) -> Vec<u8> {
    let mut output = vec![0; bytes.len()];
    let samples = bytes.len() / 2;
    for i in 0..samples {
        output[i] = bytes[2 * i];
        output[samples + i] = bytes[2 * i + 1];
    }
    output
}

#[test]
fn compressed_xisf_typed_reader_preserves_exact_uint16_for_standard_codecs_and_shuffle() {
    let mut values = [0u16; 256];
    values[0] = 65535;
    values[1] = 32768;
    values[127] = 1;
    values[255] = 0x1234;
    let raw: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    for codec in ["zstd", "zlib", "lz4"] {
        for shuffled in [false, true] {
            let input = if shuffled {
                shuffle_u16(&raw)
            } else {
                raw.clone()
            };
            let stored = encode(codec, &input);
            let compression = if shuffled {
                format!("{codec}+sh:512:2")
            } else {
                format!("{codec}:512")
            };
            let xml = format!(
                "<xisf version=\"1.0\"><Image id=\"LIGHT\" geometry=\"256:1:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:{}\" compression=\"{compression}\"/></xisf>",
                stored.len()
            );
            let fixture = write_xisf_fixture(&xml, &stored);
            let found = inventory(&fixture.0, InventoryLimits::default()).unwrap();
            let mut output = [0u16; 256];
            for (index, chunk) in output.chunks_mut(63).enumerate() {
                read_u16_chunk(
                    &fixture.0,
                    &found,
                    0,
                    (index * 63) as u64,
                    chunk,
                    PixelReadLimits::default(),
                )
                .unwrap();
            }
            assert_eq!(output, values, "{codec} shuffled={shuffled}");
        }
    }
}

#[test]
fn fits_inventory_lists_both_images_and_raw_hdu_cards() {
    let fixture = Fixture::path("fits");
    astro_io::fits::backend::with_cfitsio(|| {
        let description = ImageDescription {
            data_type: ImageType::UnsignedShort,
            dimensions: &[2, 2],
        };
        let mut file = FitsFile::create(&fixture.0)
            .with_custom_primary(&description)
            .open()
            .unwrap();
        file.primary_hdu()
            .unwrap()
            .write_image(&mut file, &[0u16, 1, 32768, 65535])
            .unwrap();
        let extension = file.create_image("SCI".to_owned(), &description).unwrap();
        extension
            .write_image(&mut file, &[10u16, 20, 30, 40])
            .unwrap();
    });
    let found = inventory(&fixture.0, InventoryLimits::default()).unwrap();
    assert_eq!(found.format, ContainerFormat::Fits);
    assert_eq!(found.images.len(), 2);
    assert_eq!(found.fits_hdus.len(), 2);
    assert_eq!(found.images[0].sample_format, "UInt16");
    assert_eq!(found.images[1].id.as_deref(), Some("SCI"));
    assert!(found.fits_hdus.iter().all(|hdu| !hdu.cards.is_empty()));
    let mut first = [0u16; 2];
    read_u16_chunk(
        &fixture.0,
        &found,
        0,
        2,
        &mut first,
        PixelReadLimits::default(),
    )
    .unwrap();
    assert_eq!(first, [32768, 65535]);
    let mut second = [0u16; 4];
    read_u16_chunk(
        &fixture.0,
        &found,
        1,
        0,
        &mut second,
        PixelReadLimits::default(),
    )
    .unwrap();
    assert_eq!(second, [10, 20, 30, 40]);
    assert!(read_u16_chunk(
        &fixture.0,
        &found,
        0,
        3,
        &mut first,
        PixelReadLimits::default()
    )
    .is_err());
}

#[test]
fn fits_inventory_counts_tile_compressed_image_as_logical_image() {
    let source = Fixture::path("fits");
    let target = Fixture::path("fits");
    let values: Vec<u16> = (0..256)
        .map(|index| if index % 17 == 0 { 65535 } else { index as u16 })
        .collect();
    astro_io::fits::backend::with_cfitsio(|| {
        let description = ImageDescription {
            data_type: ImageType::UnsignedShort,
            dimensions: &[16, 16],
        };
        let mut input = FitsFile::create(&source.0)
            .with_custom_primary(&description)
            .open()
            .unwrap();
        input
            .primary_hdu()
            .unwrap()
            .write_image(&mut input, &values)
            .unwrap();
        drop(input);
        let mut input = FitsFile::open(&source.0).unwrap();
        let mut output = FitsFile::create(&target.0).open().unwrap();
        let mut status = 0;
        // SAFETY: both native handles remain exclusively live inside the backend gate.
        unsafe {
            fitsio::sys::fits_set_compression_type(
                output.as_raw(),
                fitsio::sys::RICE_1 as i32,
                &mut status,
            );
            if status == 0 {
                fitsio::sys::fits_img_compress(input.as_raw(), output.as_raw(), &mut status);
            }
        }
        fitsio::errors::check_status(status).unwrap();
    });
    let found = inventory(&target.0, InventoryLimits::default()).unwrap();
    assert_eq!(found.images.len(), 1);
    assert_eq!(found.fits_hdus.len(), 2);
    assert_eq!(found.fits_hdus[1].kind, "compressed-image");
    assert_eq!(found.images[0].sample_format, "UInt16");
    let mut chunk = [0u16; 25];
    read_u16_chunk(
        &target.0,
        &found,
        0,
        230,
        &mut chunk,
        PixelReadLimits::default(),
    )
    .unwrap();
    assert_eq!(&chunk, &values[230..255]);
}

#[test]
fn fits_writer_roundtrips_every_image_and_supported_integer_codec() {
    let first: Vec<u16> = (0..256)
        .map(|index| if index % 13 == 0 { 65535 } else { index })
        .collect();
    let second: Vec<u16> = (0..256)
        .map(|index| if index % 19 == 0 { 32768 } else { index * 2 })
        .collect();
    let sources = [&first, &second];
    let images = [
        U16ImageSpec {
            id: None,
            width: 16,
            height: 16,
        },
        U16ImageSpec {
            id: Some("MASK"),
            width: 16,
            height: 16,
        },
    ];
    for codec in [
        FitsCodec::Uncompressed,
        FitsCodec::Rice,
        FitsCodec::Gzip1,
        FitsCodec::Gzip2,
    ] {
        let fixture = Fixture::path("fits");
        write_fits_u16(&fixture.0, &images, codec, |image, start, output| {
            output.copy_from_slice(&sources[image][start as usize..start as usize + output.len()]);
            Ok(())
        })
        .unwrap();
        let found = inventory(&fixture.0, InventoryLimits::default()).unwrap();
        assert_eq!(found.images.len(), 2, "{codec:?}");
        astro_io::fits::backend::with_cfitsio(|| {
            let mut file = FitsFile::open(&fixture.0).unwrap();
            for (index, source) in sources.iter().enumerate() {
                let hdu = file.hdu(found.images[index].source_index).unwrap();
                let pixels: Vec<u16> = hdu.read_image(&mut file).unwrap();
                assert_eq!(&pixels, *source, "{codec:?} image {index}");
            }
        });
    }
}

#[test]
fn xisf_writer_roundtrips_all_images_with_offsets_checksums_and_codecs() {
    let first: Vec<u16> = (0..256)
        .map(|index| if index % 13 == 0 { 65535 } else { index })
        .collect();
    let second: Vec<u16> = (0..256)
        .map(|index| if index % 19 == 0 { 32768 } else { index * 2 })
        .collect();
    let sources = [&first, &second];
    let images = [
        U16ImageSpec {
            id: Some("LIGHT"),
            width: 16,
            height: 16,
        },
        U16ImageSpec {
            id: Some("MASK"),
            width: 16,
            height: 16,
        },
    ];
    for codec in [
        XisfCodec::Uncompressed,
        XisfCodec::Zstd,
        XisfCodec::Zlib,
        XisfCodec::Lz4,
    ] {
        for shuffled in [false, true] {
            if codec == XisfCodec::Uncompressed && shuffled {
                continue;
            }
            let fixture = Fixture::path("xisf");
            write_xisf_u16(
                &fixture.0,
                &images,
                codec,
                shuffled,
                |image, start, output| {
                    output.copy_from_slice(
                        &sources[image][start as usize..start as usize + output.len()],
                    );
                    Ok(())
                },
            )
            .unwrap();
            let bound = xisf_output_bound_u16(&images, codec, shuffled).unwrap();
            assert!(
                fs::metadata(&fixture.0).unwrap().len() <= bound,
                "{codec:?} shuffled={shuffled}"
            );
            let found = inventory(&fixture.0, InventoryLimits::default()).unwrap();
            assert_eq!(found.images.len(), 2);
            for (image, source) in sources.iter().enumerate() {
                let mut pixels = [0u16; 256];
                for (index, chunk) in pixels.chunks_mut(61).enumerate() {
                    read_u16_chunk(
                        &fixture.0,
                        &found,
                        image,
                        (index * 61) as u64,
                        chunk,
                        PixelReadLimits::default(),
                    )
                    .unwrap();
                }
                assert_eq!(
                    pixels.as_slice(),
                    source.as_slice(),
                    "{codec:?} shuffled={shuffled} image={image}"
                );
            }
            astro_io::validation::validate_file(
                &fixture.0,
                &astro_io::validation::ValidationOptions::default()
                    .with_level(astro_io::validation::ValidationLevel::Full),
                None,
            )
            .unwrap();
        }
    }
}

#[test]
fn exact_comparator_checks_all_images_and_locates_pixel_difference() {
    let fits = Fixture::path("fits");
    let xisf = Fixture::path("xisf");
    let changed = Fixture::path("xisf");
    let images = [
        U16ImageSpec {
            id: None,
            width: 2,
            height: 2,
        },
        U16ImageSpec {
            id: Some("MASK"),
            width: 2,
            height: 2,
        },
    ];
    let source = [[0u16, 1, 32768, 65535], [11, 22, 33, 44]];
    write_fits_u16(
        &fits.0,
        &images,
        FitsCodec::Uncompressed,
        |image, start, out| {
            out.copy_from_slice(&source[image][start as usize..start as usize + out.len()]);
            Ok(())
        },
    )
    .unwrap();
    write_xisf_u16(
        &xisf.0,
        &images,
        XisfCodec::Zstd,
        false,
        |image, start, out| {
            out.copy_from_slice(&source[image][start as usize..start as usize + out.len()]);
            Ok(())
        },
    )
    .unwrap();
    write_xisf_u16(
        &changed.0,
        &images,
        XisfCodec::Zstd,
        false,
        |image, start, out| {
            out.copy_from_slice(&source[image][start as usize..start as usize + out.len()]);
            if image == 1 && start == 0 {
                out[2] = 34;
            }
            Ok(())
        },
    )
    .unwrap();
    let fi = inventory(&fits.0, InventoryLimits::default()).unwrap();
    let xi = inventory(&xisf.0, InventoryLimits::default()).unwrap();
    let ci = inventory(&changed.0, InventoryLimits::default()).unwrap();
    assert_eq!(
        compare_u16_containers(&fits.0, &fi, &xisf.0, &xi, PixelReadLimits::default()).unwrap(),
        None
    );
    assert_eq!(
        compare_u16_containers(&fits.0, &fi, &changed.0, &ci, PixelReadLimits::default()).unwrap(),
        Some(ImageMismatch::Pixel {
            image: 1,
            sample: 2,
            source: 33,
            output: 34
        })
    );
}

#[test]
fn xisf_inventory_retains_image_associations_and_ancillary_attachment() {
    let fixture = Fixture::path("xisf");
    let xml = concat!(
        "<xisf version=\"1.0\">",
        "<Image id=\"LIGHT\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\">",
        "<Property id=\"Instrument:Gain\" type=\"Float32\" value=\"100\"/>",
        "<ICCProfile location=\"attachment:4112:4\"/>",
        "</Image>",
        "<Image id=\"MASK\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4104:8\"/>",
        "</xisf>"
    );
    let mut bytes = b"XISF0100".to_vec();
    bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(xml.as_bytes());
    bytes.resize(4096, 0);
    bytes.extend_from_slice(&[0, 0, 1, 0, 0, 128, 255, 255]);
    bytes.extend_from_slice(&[10, 0, 20, 0, 30, 0, 40, 0]);
    bytes.extend_from_slice(&[0; 4]);
    fs::write(&fixture.0, bytes).unwrap();
    let found = inventory(&fixture.0, InventoryLimits::default()).unwrap();
    assert_eq!(found.format, ContainerFormat::Xisf);
    assert_eq!(found.images.len(), 2);
    assert_eq!(found.images[0].id.as_deref(), Some("LIGHT"));
    assert_eq!(found.images[1].id.as_deref(), Some("MASK"));
    let property = found
        .xisf_elements
        .iter()
        .find(|element| element.name == "Property")
        .unwrap();
    assert_eq!(property.parent_image_index, Some(0));
    let icc = found
        .xisf_elements
        .iter()
        .find(|element| element.name == "ICCProfile")
        .unwrap();
    assert_eq!(icc.parent_image_index, Some(0));
    assert_eq!(icc.attachment.unwrap().length, 4);
    assert!(found
        .xisf_xml
        .as_deref()
        .unwrap()
        .contains("Instrument:Gain"));
    let mut pixels = [0u16; 3];
    read_u16_chunk(
        &fixture.0,
        &found,
        0,
        1,
        &mut pixels,
        PixelReadLimits::default(),
    )
    .unwrap();
    assert_eq!(pixels, [1, 32768, 65535]);
    read_u16_chunk(
        &fixture.0,
        &found,
        1,
        1,
        &mut pixels,
        PixelReadLimits::default(),
    )
    .unwrap();
    assert_eq!(pixels, [20, 30, 40]);
}

#[test]
fn output_bounds_are_conservative_or_explicitly_unknown() {
    let images = [U16ImageSpec {
        id: Some("LIGHT"),
        width: 1024,
        height: 1024,
    }];
    let uncompressed = xisf_output_bound_u16(&images, XisfCodec::Uncompressed, false).unwrap();
    let compressed = xisf_output_bound_u16(&images, XisfCodec::Zstd, false).unwrap();
    assert!(compressed >= uncompressed);
    assert!(matches!(
        fits_output_bound_u16(&images, FitsCodec::Rice).unwrap(),
        EncodedSizeBound::Unknown(_)
    ));
    let oversized = [U16ImageSpec {
        id: None,
        width: usize::MAX,
        height: 2,
    }];
    assert!(xisf_output_bound_u16(&oversized, XisfCodec::Zstd, false).is_err());
}
