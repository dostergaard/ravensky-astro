use super::*;
use fitsio::sys::{GZIP_1, GZIP_2, HCOMPRESS_1, PLIO_1, RICE_1};

fn native_image(
    codec: u32,
    bitpix: i32,
    shape: [usize; 2],
    quantization: i32,
    constant: bool,
) -> Fixture {
    let [width, height] = shape;
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|i| {
            let n = if constant {
                100
            } else {
                ((i * 7919 + 31) % 65536) as i32
            };
            // Bundled CFITSIO 3.49 reserves only 4 * tile_pixels bytes for
            // PLIO encoding, omitting the line-list header in its worst case.
            // Use mask-like values here; adversarial/high-value instructions
            // are tested using independently constructed bounded streams.
            let n = if codec == PLIO_1 { n % 256 } else { n };
            match bitpix {
                8 => vec![(n % 256) as u8],
                16 => (n as i16).to_be_bytes().to_vec(),
                32 => n.to_be_bytes().to_vec(),
                -32 => (n as f32 * 0.13).to_be_bytes().to_vec(),
                -64 => (n as f64 * 0.13).to_be_bytes().to_vec(),
                _ => panic!("test BITPIX"),
            }
        })
        .collect();
    let bitpix_text = bitpix.to_string();
    let width_text = width.to_string();
    let height_text = height.to_string();
    let source = Fixture::new(&hdu(
        &[
            ("SIMPLE", "T"),
            ("BITPIX", &bitpix_text),
            ("NAXIS", "2"),
            ("NAXIS1", &width_text),
            ("NAXIS2", &height_text),
        ],
        &pixels,
    ));
    let output = Fixture::new(&[]);
    fs::remove_file(&output.0).unwrap();
    astro_io::fits::backend::with_cfitsio(|| {
        let mut source = fitsio::FitsFile::open(&source.0).unwrap();
        let mut target = fitsio::FitsFile::create(&output.0).open().unwrap();
        let mut tiles = [width as std::os::raw::c_long, 8];
        let mut status = 0;
        // SAFETY: owned file handles and fixed writable argument storage outlive
        // each call; this is test-only encoding of small, bounded valid images.
        unsafe {
            fitsio::sys::fits_set_compression_type(target.as_raw(), codec as i32, &mut status);
            fitsio::sys::fits_set_tile_dim(target.as_raw(), 2, tiles.as_mut_ptr(), &mut status);
            if bitpix < 0 {
                fitsio::sys::fits_set_quantize_level(
                    target.as_raw(),
                    if quantization == 0 { 0.0 } else { 4.0 },
                    &mut status,
                );
                if quantization != 0 {
                    fitsio::sys::fits_set_quantize_method(
                        target.as_raw(),
                        quantization,
                        &mut status,
                    );
                    fitsio::sys::fits_set_dither_seed(target.as_raw(), 1, &mut status);
                }
            }
            fitsio::sys::fits_img_compress(source.as_raw(), target.as_raw(), &mut status);
        }
        assert_eq!(status, 0, "encode codec={codec} BITPIX={bitpix}");
    });
    output
}

#[test]
fn native_integer_float_quantized_and_fallback_tiles_use_managed_admission() {
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let budget = MemoryBudget::new(4 * 1024 * 1024).unwrap();
    for codec in [GZIP_1, GZIP_2, RICE_1, PLIO_1, HCOMPRESS_1] {
        for bitpix in [8, 16, 32, -32, -64] {
            // PLIO represents only nonnegative mask values; signed 16-bit noise
            // and noninteger input are not valid PLIO encoder workloads.
            if codec == PLIO_1 && (bitpix == 16 || bitpix < 0) {
                continue;
            }
            for constant in [false, true] {
                for quantization in if bitpix < 0 { vec![-1, 1, 2] } else { vec![0] } {
                    let file = native_image(codec, bitpix, [33, 17], quantization, constant);
                    let before = fs::read(&file.0).unwrap();
                    let report = validate_file_with_budget(&file.0, &options, None, &budget)
                        .unwrap_or_else(|e| panic!("codec={codec} BITPIX={bitpix} constant={constant} quantization={quantization}: {e}"));
                    assert_eq!(report.image_count(), 1);
                    assert_eq!(before, fs::read(&file.0).unwrap());
                    assert_eq!(budget.used_bytes(), 0);
                }
            }
        }
    }
    for codec in [GZIP_1, GZIP_2] {
        for bitpix in [-32, -64] {
            let file = native_image(codec, bitpix, [33, 17], 0, false);
            validate_file_with_budget(&file.0, &options, None, &budget).unwrap();
        }
    }
}

fn compressed_table_header(bytes: &[u8]) -> (std::collections::BTreeMap<String, String>, usize) {
    let mut header = std::collections::BTreeMap::new();
    let mut position = 2880;
    loop {
        let key = std::str::from_utf8(&bytes[position..position + 8])
            .unwrap()
            .trim()
            .to_string();
        if key == "END" {
            return (header, (position + 80).div_ceil(2880) * 2880);
        }
        if &bytes[position + 8..position + 10] == b"= " {
            header.insert(
                key,
                std::str::from_utf8(&bytes[position + 10..position + 80])
                    .unwrap()
                    .split('/')
                    .next()
                    .unwrap()
                    .trim()
                    .trim_matches('\'')
                    .trim()
                    .into(),
            );
        }
        position += 80;
    }
}

#[test]
fn malformed_native_tile_streams_are_rejected_and_release_reservations() {
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let budget = MemoryBudget::new(4 * 1024 * 1024).unwrap();
    for codec in [RICE_1, PLIO_1, HCOMPRESS_1] {
        let source = native_image(codec, 32, [8, 8], 0, false);
        let original = fs::read(&source.0).unwrap();
        let (header, table) = compressed_table_header(&original);
        let row_size = header["NAXIS1"].parse::<usize>().unwrap();
        let count = i32::from_be_bytes(original[table..table + 4].try_into().unwrap()) as usize;
        let offset =
            i32::from_be_bytes(original[table + 4..table + 8].try_into().unwrap()) as usize;
        let unit = if codec == PLIO_1 { 2 } else { 1 };
        for shortened in 1..count {
            let mut bytes = original.clone();
            bytes[table..table + 4].copy_from_slice(&(shortened as i32).to_be_bytes());
            let file = Fixture::new(&bytes);
            assert!(
                validate_file_with_budget(&file.0, &options, None, &budget).is_err(),
                "codec {codec} accepted {shortened}/{count}"
            );
            assert_eq!(budget.used_bytes(), 0);
        }
        let mut bytes = original.clone();
        bytes[table..table + 4].copy_from_slice(&((count + 1) as i32).to_be_bytes());
        // Extend PCOUNT and use existing FITS padding as one extra stored element.
        let pcount = (2880..table)
            .step_by(80)
            .find(|&p| &bytes[p..p + 8] == b"PCOUNT  ")
            .unwrap();
        let new_size = header["PCOUNT"].parse::<usize>().unwrap() + unit;
        bytes[pcount..pcount + 80]
            .copy_from_slice(card("PCOUNT", &new_size.to_string()).as_bytes());
        let file = Fixture::new(&bytes);
        assert!(
            validate_file_with_budget(&file.0, &options, None, &budget).is_err(),
            "trailing codec {codec}"
        );
        if codec == HCOMPRESS_1 {
            for (relative, replacement) in [
                (2, i32::MAX.to_be_bytes().to_vec()),
                (22, vec![255, 255, 255]),
            ] {
                let mut bytes = original.clone();
                let position = table + row_size + offset + relative;
                bytes[position..position + replacement.len()].copy_from_slice(&replacement);
                let file = Fixture::new(&bytes);
                assert!(validate_file_with_budget(&file.0, &options, None, &budget).is_err());
                assert_eq!(budget.used_bytes(), 0);
            }
            let tiny = MemoryBudget::new(32768).unwrap();
            assert_eq!(
                validate_file_with_budget(&source.0, &options, None, &tiny)
                    .unwrap_err()
                    .kind(),
                ValidationErrorKind::ResourceLimit
            );
            assert_eq!(tiny.used_bytes(), 0);
        }
    }
}

fn table_fixture(
    codec: &str,
    columns: &[(&str, &str, Vec<u8>)],
    extra: &[(&str, &str)],
) -> Fixture {
    let mut row = Vec::new();
    let mut heap = Vec::new();
    let mut keys = vec![
        ("XTENSION".to_string(), "'BINTABLE'".to_string()),
        ("BITPIX".into(), "8".into()),
        ("NAXIS".into(), "2".into()),
    ];
    for (_, form, bytes) in columns {
        if form.starts_with("1P") || form.starts_with("1Q") {
            let unit = match form.as_bytes()[2] {
                b'I' => 2,
                b'J' | b'E' => 4,
                b'D' | b'K' => 8,
                _ => 1,
            };
            let count = bytes.len() / unit;
            if form.starts_with("1P") {
                row.extend_from_slice(&(count as i32).to_be_bytes());
                row.extend_from_slice(&(heap.len() as i32).to_be_bytes());
            } else {
                row.extend_from_slice(&(count as i64).to_be_bytes());
                row.extend_from_slice(&(heap.len() as i64).to_be_bytes());
            }
            heap.extend_from_slice(bytes);
        } else {
            row.extend_from_slice(bytes);
        }
    }
    keys.extend([
        ("NAXIS1".into(), row.len().to_string()),
        ("NAXIS2".into(), "1".into()),
        ("PCOUNT".into(), heap.len().to_string()),
        ("GCOUNT".into(), "1".into()),
        ("TFIELDS".into(), columns.len().to_string()),
        ("ZIMAGE".into(), "T".into()),
        ("ZBITPIX".into(), "16".into()),
        ("ZNAXIS".into(), "1".into()),
        ("ZNAXIS1".into(), "8".into()),
        ("ZCMPTYPE".into(), format!("'{codec}'")),
    ]);
    for (index, (name, form, _)) in columns.iter().enumerate() {
        keys.push((format!("TTYPE{}", index + 1), format!("'{name}'")));
        keys.push((format!("TFORM{}", index + 1), format!("'{form}'")));
    }
    keys.extend(extra.iter().map(|&(k, v)| (k.to_string(), v.to_string())));
    row.extend(heap);
    let refs: Vec<_> = keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut file = hdu(&[("SIMPLE", "T"), ("BITPIX", "8"), ("NAXIS", "0")], &[]);
    file.extend(hdu(&refs, &row));
    Fixture::new(&file)
}

#[test]
fn named_fallbacks_null_masks_and_conflicting_payloads_are_checked() {
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let budget = MemoryBudget::new(4 * 1024 * 1024).unwrap();
    for q in [false, true] {
        let descriptor = if q { "1QB" } else { "1PB" };
        for mask_codec in ["GZIP_1", "RICE_1", "PLIO_1"] {
            let mask = match mask_codec {
                "GZIP_1" => compressed_frame("gzip", &[0; 8]),
                "RICE_1" => vec![0, 0], // BYTEPIX=1 zero seed and zero-difference block.
                _ => [0i16, 7, -100, 8, 0, 0, 0, 8]
                    .into_iter()
                    .flat_map(i16::to_be_bytes)
                    .collect(),
            };
            let columns = vec![
                ("NULL_PIXEL_MASK", descriptor, mask),
                (
                    "GZIP_COMPRESSED_DATA",
                    descriptor,
                    compressed_frame("gzip", &[42; 16]),
                ),
                ("COMPRESSED_DATA", descriptor, vec![]),
            ];
            let key = format!("'{mask_codec}'");
            let file = table_fixture("RICE_1", &columns, &[("ZMASKCMP", &key)]);
            validate_file_with_budget(&file.0, &options, None, &budget).unwrap();
            let mut corrupt = columns.clone();
            corrupt[0].2.pop();
            let file = table_fixture("RICE_1", &corrupt, &[("ZMASKCMP", &key)]);
            assert!(validate_file_with_budget(&file.0, &options, None, &budget).is_err());
            let mut ambiguous = columns;
            ambiguous[2].2 = vec![0, 0];
            let file = table_fixture("RICE_1", &ambiguous, &[("ZMASKCMP", &key)]);
            assert_eq!(
                validate_file_with_budget(&file.0, &options, None, &budget)
                    .unwrap_err()
                    .kind(),
                ValidationErrorKind::InvalidStructure
            );
            assert_eq!(budget.used_bytes(), 0);
        }
        let columns = [
            (
                "UNCOMPRESSED_DATA",
                if q { "1QI" } else { "1PI" },
                vec![0; 16],
            ),
            ("COMPRESSED_DATA", descriptor, vec![]),
        ];
        let file = table_fixture("RICE_1", &columns, &[]);
        validate_file_with_budget(&file.0, &options, None, &budget).unwrap();
        let mut bad = columns.clone();
        bad[0].2.truncate(14);
        let file = table_fixture("RICE_1", &bad, &[]);
        assert_eq!(
            validate_file_with_budget(&file.0, &options, None, &budget)
                .unwrap_err()
                .kind(),
            ValidationErrorKind::IntegrityMismatch
        );
    }
}

#[test]
fn controlled_codecs_reject_mutated_streams_without_panics_or_leaked_admission() {
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let budget = MemoryBudget::new(4 * 1024 * 1024).unwrap();
    for codec in [RICE_1, PLIO_1, HCOMPRESS_1] {
        let original = native_image(codec, 32, [8, 8], 0, false);
        let bytes = fs::read(&original.0).unwrap();
        let (header, table) = compressed_table_header(&bytes);
        let heap = table + header["NAXIS1"].parse::<usize>().unwrap();
        let size = header["PCOUNT"].parse::<usize>().unwrap();
        for index in 0..size {
            let mut mutated = bytes.clone();
            mutated[heap + index] ^= 0xff;
            let fixture = Fixture::new(&mutated);
            // Some mutations are another valid stream when no checksum exists.
            // The contract here is a bounded result without panic or leaked state.
            let _ = validate_file_with_budget(&fixture.0, &options, None, &budget);
            assert_eq!(budget.used_bytes(), 0);
        }
    }
}

#[test]
fn busy_admission_cancellation_and_concurrent_codecs_release_shared_resources() {
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let budget = MemoryBudget::new(4 * 1024 * 1024).unwrap();
    let files: Vec<_> = [RICE_1, PLIO_1, HCOMPRESS_1]
        .into_iter()
        .map(|codec| native_image(codec, 32, [64, 64], 0, false))
        .collect();
    let held = budget.try_reserve(budget.capacity_bytes() - 4096).unwrap();
    for file in &files {
        assert_eq!(
            validate_file_with_budget(&file.0, &options, None, &budget)
                .unwrap_err()
                .kind(),
            ValidationErrorKind::ResourceBusy
        );
        assert_eq!(budget.used_bytes(), budget.capacity_bytes() - 4096);
    }
    drop(held);
    let cancel = AtomicBool::new(true);
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let options = &options;
            let budget = &budget;
            let cancel = &cancel;
            workers.push(scope.spawn(move || {
                for _ in 0..8 {
                    let result = validate_file_with_budget(
                        &file.0,
                        options,
                        if index == 0 { Some(cancel) } else { None },
                        budget,
                    );
                    if index == 0 {
                        assert_eq!(result.unwrap_err().kind(), ValidationErrorKind::Cancelled);
                    } else {
                        result.unwrap();
                    }
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    assert_eq!(budget.used_bytes(), 0);
    assert!(budget.peak_bytes() <= budget.capacity_bytes());
}

#[test]
fn bounded_random_hcompress_streams_and_geometry_never_panic() {
    let budget = MemoryBudget::new(2 * 1024 * 1024).unwrap();
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let mut seed = 0x12345678u32;
    for case in 0..1000 {
        let width = case % 17 + 1;
        let height = case % 13 + 1;
        let mut frame: Vec<u8> = (0..128)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        frame[..2].copy_from_slice(&[0xdd, 0x99]);
        frame[2..6].copy_from_slice(&(height as i32).to_be_bytes());
        frame[6..10].copy_from_slice(&(width as i32).to_be_bytes());
        frame[10..14].copy_from_slice(&1i32.to_be_bytes());
        for v in &mut frame[22..25] {
            *v %= 33;
        }
        let file = Fixture::new(&gzip_fits(
            &[frame],
            &[width, height],
            &[width, height],
            32,
            false,
            "HCOMPRESS_1",
        ));
        let _ = validate_file_with_budget(&file.0, &options, None, &budget);
        assert_eq!(budget.used_bytes(), 0);
    }
}
