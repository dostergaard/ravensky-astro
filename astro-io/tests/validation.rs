use astro_io::validation::{
    validate_file, validate_file_with_budget, MemoryBudget, ValidationErrorKind, ValidationLevel,
    ValidationLimits, ValidationOptions,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "astro-validation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

fn compressed_frame(codec: &str, data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    match codec {
        "gzip" => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(data).unwrap();
            encoder.finish().unwrap()
        }
        "zlib" => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(data).unwrap();
            encoder.finish().unwrap()
        }
        "zstd" => {
            let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 1).unwrap();
            encoder.window_log(17).unwrap();
            encoder.write_all(data).unwrap();
            encoder.finish().unwrap()
        }
        _ => lz4_flex::block::compress(data),
    }
}

fn gzip_fits(
    frames: &[Vec<u8>],
    axes: &[usize],
    tiles: &[usize],
    bitpix: i32,
    q: bool,
    codec: &str,
) -> Vec<u8> {
    let row = if q { 16 } else { 8 };
    let gap = 17;
    let mut table = Vec::new();
    let mut heap = vec![0; gap];
    for frame in frames {
        let offset = heap.len() - gap;
        if q {
            table.extend_from_slice(&(frame.len() as i64).to_be_bytes());
            table.extend_from_slice(&(offset as i64).to_be_bytes());
        } else {
            table.extend_from_slice(&(frame.len() as i32).to_be_bytes());
            table.extend_from_slice(&(offset as i32).to_be_bytes());
        }
        heap.extend_from_slice(frame);
    }
    let mut keys: Vec<(String, String)> =
        [("XTENSION", "'BINTABLE'"), ("BITPIX", "8"), ("NAXIS", "2")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
    for (key, value) in [
        ("NAXIS1", row),
        ("NAXIS2", frames.len()),
        ("PCOUNT", heap.len()),
        ("GCOUNT", 1),
        ("TFIELDS", 1),
        ("THEAP", table.len() + gap),
        ("ZNAXIS", axes.len()),
    ] {
        keys.push((key.into(), value.to_string()));
    }
    keys.extend([
        ("TTYPE1".into(), "'COMPRESSED_DATA'".into()),
        ("TFORM1".into(), if q { "'1QB'" } else { "'1PB'" }.into()),
        ("ZIMAGE".into(), "T".into()),
        ("ZCMPTYPE".into(), format!("'{codec}'")),
        ("ZBITPIX".into(), bitpix.to_string()),
    ]);
    for (i, n) in axes.iter().enumerate() {
        keys.push((format!("ZNAXIS{}", i + 1), n.to_string()));
    }
    for (i, n) in tiles.iter().enumerate() {
        keys.push((format!("ZTILE{}", i + 1), n.to_string()));
    }
    let refs: Vec<_> = keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut bytes = hdu(&[("SIMPLE", "T"), ("BITPIX", "8"), ("NAXIS", "0")], &[]);
    table.extend(heap);
    bytes.extend(hdu(&refs, &table));
    bytes
}

#[test]
fn fits_gzip_streams_large_tiles_with_a_small_shared_budget() {
    let payload = vec![42; 8 * 1024 * 1024];
    let frame = compressed_frame("gzip", &payload);
    let budget = MemoryBudget::new(2 * 1024 * 1024).unwrap();
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    for codec in ["GZIP_1", "GZIP_2"] {
        for q in [false, true] {
            let file = Fixture::new(&gzip_fits(
                std::slice::from_ref(&frame),
                &[payload.len()],
                &[],
                8,
                q,
                codec,
            ));
            let report = validate_file_with_budget(&file.0, &options, None, &budget).unwrap();
            assert_eq!(report.image_count(), 1);
            assert!(report.peak_reserved_bytes() < budget.capacity_bytes());
            assert_eq!(budget.used_bytes(), 0);
        }
    }
    // A highly compressible payload must not expand beyond the tile declaration.
    let concurrent = Fixture::new(&gzip_fits(
        std::slice::from_ref(&frame),
        &[payload.len()],
        &[],
        8,
        false,
        "GZIP_1",
    ));
    let shared = MemoryBudget::new(8 * 1024 * 1024).unwrap();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    validate_file_with_budget(&concurrent.0, &options, None, &shared).unwrap()
                })
            })
            .collect();
        for worker in workers {
            assert_eq!(worker.join().unwrap().image_count(), 1);
        }
    });
    assert_eq!(shared.used_bytes(), 0);
    assert!(shared.peak_bytes() <= shared.capacity_bytes());
    let file = Fixture::new(&gzip_fits(&[frame], &[128], &[], 8, false, "GZIP_1"));
    assert_eq!(
        validate_file_with_budget(&file.0, &options, None, &budget)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::IntegrityMismatch
    );
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn fits_gzip_checks_edges_trailers_headers_and_shared_admission() {
    use std::io::Write;
    let budget = MemoryBudget::new(2 * 1024 * 1024).unwrap();
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    for bitpix in [8, 16, 32, 64] {
        let frames: Vec<_> = [6, 4, 3, 2, 6, 4, 3, 2]
            .iter()
            .map(|n| compressed_frame("gzip", &vec![42; n * bitpix as usize / 8]))
            .collect();
        let file = Fixture::new(&gzip_fits(
            &frames,
            &[5, 3, 2],
            &[3, 2, 1],
            bitpix,
            true,
            "GZIP_2",
        ));
        validate_file_with_budget(&file.0, &options, None, &budget).unwrap();
        assert_eq!(budget.used_bytes(), 0);
    }
    let frame = compressed_frame("gzip", &[42; 128]);
    let mut encoder = flate2::GzBuilder::new()
        .filename("capture.fits")
        .comment("synthetic fixture")
        .extra(vec![0; 4096])
        .write(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&[42; 128]).unwrap();
    let named = Fixture::new(&gzip_fits(
        &[encoder.finish().unwrap()],
        &[128],
        &[],
        8,
        false,
        "GZIP_1",
    ));
    validate_file_with_budget(&named.0, &options, None, &budget).unwrap();
    let check = |bytes: Vec<u8>, expected_kind| {
        let file = Fixture::new(&gzip_fits(&[bytes], &[128], &[], 8, false, "GZIP_1"));
        validate_file(&file.0, &ValidationOptions::default(), None).unwrap();
        assert_eq!(
            validate_file_with_budget(&file.0, &options, None, &budget)
                .unwrap_err()
                .kind(),
            expected_kind
        );
        assert_eq!(budget.used_bytes(), 0);
    };
    for trim in 1..=9 {
        check(
            frame[..frame.len() - trim].to_vec(),
            ValidationErrorKind::IntegrityMismatch,
        );
    }
    for at in [frame.len() - 8, frame.len() - 4] {
        let mut corrupt = frame.clone();
        corrupt[at] ^= 1;
        check(corrupt, ValidationErrorKind::IntegrityMismatch);
    }
    let mut trailing = frame.clone();
    trailing.push(0);
    check(trailing, ValidationErrorKind::IntegrityMismatch);
    let mut concatenated = frame.clone();
    concatenated.extend(&frame);
    check(concatenated, ValidationErrorKind::Unsupported);
    check(
        compressed_frame("gzip", &[42; 127]),
        ValidationErrorKind::IntegrityMismatch,
    );
    // An unterminated filename must be bounded before the library header parser
    // can retain the rest of a file in its filename Vec.
    let mut header = vec![b'a'; 80 * 1024];
    header[..10].copy_from_slice(&[31, 139, 8, 8, 0, 0, 0, 0, 0, 255]);
    check(header, ValidationErrorKind::ResourceLimit);
    let mut invalid = frame.clone();
    invalid[0] = 0;
    check(invalid, ValidationErrorKind::IntegrityMismatch);
    // A non-final Deflate block followed by a plausible GZIP trailer must fail.
    let mut nonfinal = frame[..10].to_vec();
    nonfinal.extend([0, 128, 0, 127, 255]);
    nonfinal.extend([42; 128]);
    nonfinal.extend(&frame[frame.len() - 8..]);
    check(nonfinal, ValidationErrorKind::IntegrityMismatch);
    let file = Fixture::new(&gzip_fits(&[frame], &[128], &[], 8, false, "GZIP_1"));
    let held = budget.try_reserve(1024 * 1024).unwrap();
    assert_eq!(
        validate_file_with_budget(&file.0, &options, None, &budget)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::ResourceBusy
    );
    assert_eq!(budget.used_bytes(), held.bytes());
    drop(held);
    let cancel = AtomicBool::new(true);
    assert_eq!(
        validate_file_with_budget(&file.0, &options, Some(&cancel), &budget)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::Cancelled
    );
    assert_eq!(budget.used_bytes(), 0);
    let small = MemoryBudget::new(1024 * 1024).unwrap();
    assert_eq!(
        validate_file_with_budget(&file.0, &options, None, &small)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::ResourceLimit
    );
    assert_eq!(small.used_bytes(), 0);
}

#[test]
fn fits_gzip_managed_profile_does_not_admit_other_native_layouts() {
    let frame = compressed_frame("gzip", &[42; 128]);
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let budget = MemoryBudget::new(2 * 1024 * 1024).unwrap();
    for (bitpix, codec) in [(-32, "GZIP_1"), (8, "RICE_1"), (8, "HCOMPRESS_1")] {
        let file = Fixture::new(&gzip_fits(
            std::slice::from_ref(&frame),
            &[32],
            &[],
            bitpix,
            false,
            codec,
        ));
        assert_eq!(
            validate_file_with_budget(&file.0, &options, None, &budget)
                .unwrap_err()
                .kind(),
            ValidationErrorKind::Unsupported
        );
        assert_eq!(budget.used_bytes(), 0);
    }
    let mut bytes = gzip_fits(&[frame], &[128], &[], 8, false, "GZIP_1");
    let fields = (2880..5760)
        .step_by(80)
        .find(|&p| &bytes[p..p + 8] == b"TFIELDS ")
        .unwrap();
    bytes[fields..fields + 80].copy_from_slice(card("TFIELDS", "2").as_bytes());
    let end = (2880..5760)
        .step_by(80)
        .find(|&p| &bytes[p..p + 8] == b"END     ")
        .unwrap();
    bytes.copy_within(end..end + 80, end + 160);
    bytes[end..end + 80].copy_from_slice(card("TFORM2", "'0B'").as_bytes());
    bytes[end + 80..end + 160].copy_from_slice(card("TTYPE2", "'OTHER'").as_bytes());
    let file = Fixture::new(&bytes);
    validate_file(&file.0, &ValidationOptions::default(), None).unwrap();
    assert_eq!(
        validate_file_with_budget(&file.0, &options, None, &budget)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::Unsupported
    );
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn fits_checksums_are_verified_before_compressed_payload_decode() {
    let mut frame = compressed_frame("gzip", &[42; 128]);
    frame[0] = 0;
    let mut bytes = gzip_fits(&[frame], &[128], &[], 8, false, "GZIP_1");
    let end = (2880..5760)
        .step_by(80)
        .find(|&p| &bytes[p..p + 8] == b"END     ")
        .unwrap();
    bytes.copy_within(end..end + 80, end + 80);
    bytes[end..end + 80].copy_from_slice(card("DATASUM", "'1'").as_bytes());
    let file = Fixture::new(&bytes);
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let error = validate_file_with_budget(
        &file.0,
        &options,
        None,
        &MemoryBudget::new(2 * 1024 * 1024).unwrap(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("DATASUM mismatch"), "{error}");
}
fn compressed_fixture(codec: &str, bytes: &[u8], decoded: usize) -> Fixture {
    let xml = format!(
        r#"<xisf version="1.0"><Image geometry="{decoded}:1:1" sampleFormat="UInt8" location="attachment:4096:{}" compression="{codec}:{decoded}"/></xisf>"#,
        bytes.len()
    );
    Fixture::new(&xisf(&xml, bytes))
}
#[test]
fn large_streamed_payloads_fit_small_shared_budgets_and_release_on_every_exit() {
    let budget = MemoryBudget::new(2 * 1024 * 1024).unwrap();
    let options = ValidationOptions::default()
        .with_level(ValidationLevel::Full)
        .with_limits(
            ValidationLimits::default()
                .with_max_working_bytes(2 * 1024 * 1024)
                .unwrap(),
        );
    let data = vec![42; 8 * 1024 * 1024];
    for codec in ["zlib", "zstd"] {
        let compressed = compressed_frame(codec, &data);
        let file = compressed_fixture(codec, &compressed, data.len());
        let report = validate_file_with_budget(&file.0, &options, None, &budget).unwrap();
        assert!(report.peak_reserved_bytes() < 2 * 1024 * 1024);
        assert_eq!(budget.used_bytes(), 0);
        let occupied = budget.try_reserve(1024 * 1024).unwrap();
        assert_eq!(
            validate_file_with_budget(&file.0, &options, None, &budget)
                .unwrap_err()
                .kind(),
            ValidationErrorKind::ResourceBusy
        );
        assert_eq!(budget.used_bytes(), occupied.bytes());
        drop(occupied);
        let cancel = std::sync::atomic::AtomicBool::new(true);
        assert_eq!(
            validate_file_with_budget(&file.0, &options, Some(&cancel), &budget)
                .unwrap_err()
                .kind(),
            ValidationErrorKind::Cancelled
        );
        assert_eq!(budget.used_bytes(), 0);
        for expected in [data.len() - 1, data.len() + 1] {
            let bad = compressed_fixture(codec, &compressed, expected);
            assert_eq!(
                validate_file_with_budget(&bad.0, &options, None, &budget)
                    .unwrap_err()
                    .kind(),
                ValidationErrorKind::IntegrityMismatch
            );
            assert_eq!(budget.used_bytes(), 0);
        }
    }
    let compressed = compressed_frame("lz4", &data);
    let file = compressed_fixture("lz4", &compressed, data.len());
    assert_eq!(
        validate_file_with_budget(&file.0, &options, None, &budget)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::ResourceLimit
    );
    assert_eq!(budget.used_bytes(), 0);
}
#[test]
fn streaming_rejects_truncated_footers_and_trailing_bytes() {
    let budget = MemoryBudget::new(4 * 1024 * 1024).unwrap();
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let data = vec![23; 128 * 1024];
    for codec in ["zlib", "zstd"] {
        let original = compressed_frame(codec, &data);
        for cut in 1..=4 {
            let file = compressed_fixture(codec, &original[..original.len() - cut], data.len());
            assert_eq!(
                validate_file_with_budget(&file.0, &options, None, &budget)
                    .unwrap_err()
                    .kind(),
                ValidationErrorKind::IntegrityMismatch,
                "{codec} cut {cut}"
            );
            assert_eq!(budget.used_bytes(), 0);
        }
        let mut trailing = original;
        trailing.extend([1, 2, 3, 4]);
        let file = compressed_fixture(codec, &trailing, data.len());
        assert_eq!(
            validate_file_with_budget(&file.0, &options, None, &budget)
                .unwrap_err()
                .kind(),
            ValidationErrorKind::IntegrityMismatch
        );
        assert_eq!(budget.used_bytes(), 0);
    }
}
#[test]
fn zstd_concatenated_and_skippable_frames_are_accounted_independently() {
    let data = vec![23; 128 * 1024];
    let frame = compressed_frame("zstd", &data);
    let mut payload = frame.clone();
    payload.extend(0x184d2a50u32.to_le_bytes());
    payload.extend(3u32.to_le_bytes());
    payload.extend([1, 2, 3]);
    payload.extend(frame);
    let file = compressed_fixture("zstd", &payload, data.len() * 2);
    let budget = MemoryBudget::new(2 * 1024 * 1024).unwrap();
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    validate_file_with_budget(&file.0, &options, None, &budget).unwrap();
    assert_eq!(budget.used_bytes(), 0);
    // Valid magic with an excessive declared window: refuse before native decode.
    let file = compressed_fixture("zstd", &[0x28, 0xb5, 0x2f, 0xfd, 0, 0xf8, 1, 0, 0], 1);
    assert_eq!(
        validate_file_with_budget(&file.0, &options, None, &budget)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::ResourceLimit
    );
    assert_eq!(budget.used_bytes(), 0);
}
#[test]
fn concurrent_validations_share_one_budget_and_leave_no_reservations() {
    let data = vec![19; 1024 * 1024];
    let file = compressed_fixture("zlib", &compressed_frame("zlib", &data), data.len());
    let budget = MemoryBudget::new(8 * 1024 * 1024).unwrap();
    let options = ValidationOptions::default().with_level(ValidationLevel::Full);
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let file = &file;
            let budget = &budget;
            let options = &options;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                validate_file_with_budget(&file.0, options, None, budget).unwrap();
            });
        }
    });
    assert_eq!(budget.used_bytes(), 0);
    assert!(budget.peak_bytes() <= budget.capacity_bytes());
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn card(key: &str, value: &str) -> String {
    format!("{key:<8}= {value:>20}")
        .chars()
        .chain(std::iter::repeat(' '))
        .take(80)
        .collect()
}
fn fits() -> Vec<u8> {
    let mut bytes = [
        card("SIMPLE", "T"),
        card("BITPIX", "16"),
        card("NAXIS", "2"),
        card("NAXIS1", "2"),
        card("NAXIS2", "2"),
        format!("{:<80}", "END"),
    ]
    .concat()
    .into_bytes();
    bytes.resize(2880, b' ');
    bytes.extend([0, 1, 0, 2, 0, 3, 0, 4]);
    bytes.resize(5760, 0);
    bytes
}
#[test]
fn fits_is_valid_at_both_levels_and_not_modified() {
    let bytes = fits();
    let file = Fixture::new(&bytes);
    for level in [ValidationLevel::Structural, ValidationLevel::Full] {
        let report = validate_file(
            &file.0,
            &ValidationOptions::default().with_level(level),
            None,
        )
        .unwrap();
        assert_eq!(report.level(), level);
        assert_eq!(report.image_count(), 1);
    }
    assert_eq!(fs::read(&file.0).unwrap(), bytes);
}
#[test]
fn missing_image_bytes_are_incomplete_even_with_readable_header() {
    let file = Fixture::new(&fits()[..2884]);
    assert_eq!(
        validate_file(&file.0, &ValidationOptions::default(), None)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::Incomplete
    );
}
#[test]
fn cancellation_is_a_distinct_outcome() {
    let file = Fixture::new(&fits());
    assert_eq!(
        validate_file(
            &file.0,
            &ValidationOptions::default(),
            Some(&AtomicBool::new(true))
        )
        .unwrap_err()
        .kind(),
        ValidationErrorKind::Cancelled
    );
}
fn xisf(xml: &str, data: &[u8]) -> Vec<u8> {
    let mut bytes = b"XISF0100".to_vec();
    bytes.extend((xml.len() as u32).to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend(xml.as_bytes());
    bytes.resize(4096, 0);
    bytes.extend(data);
    bytes
}
#[test]
fn xisf_rgb_float_layout_is_supported_without_using_narrow_pixel_loader() {
    let file = Fixture::new(&xisf(
        r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:3" sampleFormat="Float32" colorSpace="RGB" location="attachment:4096:48"/></xisf>"#,
        &[0; 48],
    ));
    assert_eq!(
        validate_file(&file.0, &ValidationOptions::default(), None)
            .unwrap()
            .image_count(),
        1
    );
}
#[test]
fn xisf_auxiliary_truncation_prevents_success() {
    let file = Fixture::new(&xisf(
        r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:4096:8"><ICCProfile location="attachment:4104:20"/></Image></xisf>"#,
        &[0; 8],
    ));
    assert_eq!(
        validate_file(&file.0, &ValidationOptions::default(), None)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::Incomplete
    );
}
fn hdu(fields: &[(&str, &str)], data: &[u8]) -> Vec<u8> {
    let mut h = fields.iter().map(|(k, v)| card(k, v)).collect::<String>();
    h.push_str(&format!("{:<80}", "END"));
    let mut b = h.into_bytes();
    b.resize(b.len().div_ceil(2880) * 2880, b' ');
    b.extend(data);
    b.resize(b.len().div_ceil(2880) * 2880, 0);
    b
}
#[test]
fn fits_ascii_and_binary_extensions_are_validated() {
    let primary = hdu(&[("SIMPLE", "T"), ("BITPIX", "8"), ("NAXIS", "0")], &[]);
    let ascii = hdu(
        &[
            ("XTENSION", "'TABLE'"),
            ("BITPIX", "8"),
            ("NAXIS", "2"),
            ("NAXIS1", "4"),
            ("NAXIS2", "1"),
            ("PCOUNT", "0"),
            ("GCOUNT", "1"),
            ("TFIELDS", "1"),
            ("TFORM1", "'I4'"),
            ("TBCOL1", "1"),
        ],
        b" 123",
    );
    let binary = hdu(
        &[
            ("XTENSION", "'BINTABLE'"),
            ("BITPIX", "8"),
            ("NAXIS", "2"),
            ("NAXIS1", "8"),
            ("NAXIS2", "1"),
            ("PCOUNT", "4"),
            ("GCOUNT", "1"),
            ("TFIELDS", "1"),
            ("TFORM1", "'1PB(4)'"),
        ],
        &[0, 0, 0, 4, 0, 0, 0, 0, 1, 2, 3, 4],
    );
    let mut bytes = primary;
    bytes.extend(ascii);
    bytes.extend(binary);
    let file = Fixture::new(&bytes);
    validate_file(
        &file.0,
        &ValidationOptions::default().with_level(ValidationLevel::Full),
        None,
    )
    .unwrap();
    bytes.truncate(bytes.len() - 2880);
    let truncated = Fixture::new(&bytes);
    assert_eq!(
        validate_file(&truncated.0, &ValidationOptions::default(), None)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::Incomplete
    );
}
#[test]
fn checksum_validation_distinguishes_same_length_corruption() {
    astro_io::fits::backend::with_cfitsio(|| {
        let file = Fixture::new(&fits());
        {
            let mut f = fitsio::FitsFile::edit(&file.0).unwrap();
            let mut status = 0;
            // SAFETY: f owns a live writable handle and status is a valid out-pointer.
            unsafe {
                fitsio::sys::ffpcks(f.as_raw(), &mut status);
            }
            assert_eq!(status, 0);
        }
        let options = ValidationOptions::default().with_level(ValidationLevel::Full);
        let report = validate_file(&file.0, &options, None).unwrap();
        assert_eq!(report.checksums().verified(), 2);
        let mut bytes = fs::read(&file.0).unwrap();
        bytes[2881] ^= 1;
        fs::write(&file.0, bytes).unwrap();
        validate_file(&file.0, &ValidationOptions::default(), None).unwrap();
        assert_eq!(
            validate_file(&file.0, &options, None).unwrap_err().kind(),
            ValidationErrorKind::IntegrityMismatch
        );
    })
}
#[test]
fn all_xisf_capture_codecs_and_shuffling_validate() {
    use std::io::Write;
    let data: Vec<u8> = (0..64).collect();
    for codec in ["zlib", "lz4", "lz4hc", "zstd"] {
        for shuffled in [false, true] {
            let input = if shuffled {
                (0..2)
                    .flat_map(|i| data.iter().skip(i).step_by(2).copied())
                    .collect::<Vec<_>>()
            } else {
                data.clone()
            };
            let compressed = match codec {
                "zlib" => {
                    let mut e =
                        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                    e.write_all(&input).unwrap();
                    e.finish().unwrap()
                }
                "lz4" | "lz4hc" => lz4_flex::block::compress(&input),
                _ => zstd::stream::encode_all(std::io::Cursor::new(&input), 1).unwrap(),
            };
            let compression = if shuffled {
                format!("{codec}+sh:64:2")
            } else {
                format!("{codec}:64")
            };
            let xml = format!(
                r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="8:4:1" sampleFormat="UInt16" location="attachment:4096:{}" compression="{}"/></xisf>"#,
                compressed.len(),
                compression
            );
            let file = Fixture::new(&xisf(&xml, &compressed));
            validate_file(
                &file.0,
                &ValidationOptions::default().with_level(ValidationLevel::Full),
                None,
            )
            .unwrap();
        }
    }
}
#[test]
fn published_sha_vectors_validate_inline_payloads() {
    for (algorithm,digest) in [
        ("sha1","a9993e364706816aba3e25717850c26c9cd0d89d"),
        ("sha-256","ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        ("sha512","ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"),
        ("sha3-256","3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"),
        ("sha3-512","b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0"),
    ] {
        let xml=format!(r#"<?xml version="1.0"?><xisf version="1.0"><Property id="data" type="ByteArray" length="3" location="inline:base64" checksum="{algorithm}:{digest}">YWJj</Property></xisf>"#);
        let file=Fixture::new(&xisf(&xml,&[]));let report=validate_file(&file.0,&ValidationOptions::default().with_level(ValidationLevel::Full),None).unwrap();assert_eq!(report.checksums().verified(),1);
    }
}
#[test]
fn unknown_codec_is_structural_only_and_never_full_success() {
    let xml = r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:4096:8" compression="future:8"/></xisf>"#;
    let file = Fixture::new(&xisf(xml, &[0; 8]));
    validate_file(&file.0, &ValidationOptions::default(), None).unwrap();
    assert_eq!(
        validate_file(
            &file.0,
            &ValidationOptions::default().with_level(ValidationLevel::Full),
            None
        )
        .unwrap_err()
        .kind(),
        ValidationErrorKind::Unsupported
    );
}
#[test]
fn configurable_limits_and_oversized_lengths_fail_explicitly() {
    use astro_io::validation::ValidationLimits;
    let file = Fixture::new(&fits());
    for limits in [
        ValidationLimits::default()
            .with_max_header_bytes(80)
            .unwrap(),
        ValidationLimits::default().with_max_structures(1).unwrap(),
        ValidationLimits::default()
            .with_max_decoded_bytes(1)
            .unwrap(),
        ValidationLimits::default()
            .with_max_working_bytes(1)
            .unwrap(),
    ] {
        assert_eq!(
            validate_file(
                &file.0,
                &ValidationOptions::default().with_limits(limits),
                None
            )
            .unwrap_err()
            .kind(),
            ValidationErrorKind::ResourceLimit
        );
    }
    let xml = r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="18446744073709551615:2:1" sampleFormat="UInt16" location="attachment:4096:8"/></xisf>"#;
    let file = Fixture::new(&xisf(xml, &[0; 8]));
    assert_eq!(
        validate_file(&file.0, &ValidationOptions::default(), None)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::InvalidStructure
    );
}
#[test]
fn malformed_xml_reserved_bytes_and_external_blocks_do_not_pass() {
    for (xml, kind) in [
        (
            r#"<?xml version="1.0"?><xisf version="1.0"><Image></xisf>"#,
            ValidationErrorKind::InvalidStructure,
        ),
        (
            r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt16" location="url(http://example.com/pixels)"/></xisf>"#,
            ValidationErrorKind::Unsupported,
        ),
        (
            r#"<?xml version="1.0"?><!DOCTYPE xisf [<!ENTITY test SYSTEM "file:///etc/passwd">]><xisf version="1.0"/>"#,
            ValidationErrorKind::Unsupported,
        ),
    ] {
        let file = Fixture::new(&xisf(xml, &[]));
        assert_eq!(
            validate_file(&file.0, &ValidationOptions::default(), None)
                .unwrap_err()
                .kind(),
            kind
        );
    }
    let mut bytes = xisf(r#"<?xml version="1.0"?><xisf version="1.0"/>"#, &[]);
    bytes[12] = 1;
    let file = Fixture::new(&bytes);
    assert_eq!(
        validate_file(&file.0, &ValidationOptions::default(), None)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::InvalidStructure
    );
}
#[test]
fn tiled_fits_is_checked_through_the_native_backend() {
    astro_io::fits::backend::with_cfitsio(|| {
        use fitsio::sys::{GZIP_1, GZIP_2, HCOMPRESS_1, PLIO_1, RICE_1};
        let source = Fixture::new(&hdu(
            &[
                ("SIMPLE", "T"),
                ("BITPIX", "16"),
                ("NAXIS", "2"),
                ("NAXIS1", "16"),
                ("NAXIS2", "16"),
            ],
            &[0; 512],
        ));
        for codec in [RICE_1, GZIP_1, GZIP_2, PLIO_1, HCOMPRESS_1] {
            let target = Fixture::new(&[]);
            fs::remove_file(&target.0).unwrap();
            {
                let mut input = fitsio::FitsFile::open(&source.0).unwrap();
                let mut output = fitsio::FitsFile::create(&target.0).open().unwrap();
                let mut status = 0;
                // SAFETY: independent live handles are owned for the full call, status is writable.
                unsafe {
                    fitsio::sys::fits_set_compression_type(
                        output.as_raw(),
                        codec as i32,
                        &mut status,
                    );
                    fitsio::sys::fits_img_compress(input.as_raw(), output.as_raw(), &mut status);
                }
                assert_eq!(status, 0, "encoding codec {codec}");
            }
            for level in [ValidationLevel::Structural, ValidationLevel::Full] {
                let report = validate_file(
                    &target.0,
                    &ValidationOptions::default().with_level(level),
                    None,
                )
                .unwrap();
                assert_eq!(report.image_count(), 1);
                let budget = MemoryBudget::new(8 * 1024 * 1024).unwrap();
                let controlled = validate_file_with_budget(
                    &target.0,
                    &ValidationOptions::default().with_level(level),
                    None,
                    &budget,
                );
                if level == ValidationLevel::Full && codec != GZIP_1 && codec != GZIP_2 {
                    assert_eq!(
                        controlled.unwrap_err().kind(),
                        ValidationErrorKind::Unsupported
                    );
                } else {
                    controlled.unwrap();
                }
                assert_eq!(budget.used_bytes(), 0);
            }
            let mut bytes = fs::read(&target.0).unwrap();
            let tile = bytes.windows(8).position(|b| b == b"ZTILE1  ").unwrap();
            bytes[tile..tile + 80].copy_from_slice(card("ZTILE1", "0").as_bytes());
            fs::write(&target.0, bytes).unwrap();
            assert_eq!(
                validate_file(&target.0, &ValidationOptions::default(), None)
                    .unwrap_err()
                    .kind(),
                ValidationErrorKind::InvalidStructure
            );
        }
    })
}
#[test]
fn embedded_hex_and_multiple_compression_subblocks_are_supported() {
    let file = Fixture::new(&xisf(
        r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt8" location="embedded"><Data encoding="hex">01 02 03 04</Data></Image></xisf>"#,
        &[],
    ));
    validate_file(
        &file.0,
        &ValidationOptions::default().with_level(ValidationLevel::Full),
        None,
    )
    .unwrap();
    let a = lz4_flex::block::compress(&[1, 2, 3, 4]);
    let b = lz4_flex::block::compress(&[5, 6, 7, 8]);
    let xml = format!(
        r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:4096:{}" compression="lz4:8" subblocks="{},4:{},4"/></xisf>"#,
        a.len() + b.len(),
        a.len(),
        b.len()
    );
    let mut data = a;
    data.extend(b);
    let file = Fixture::new(&xisf(&xml, &data));
    validate_file(
        &file.0,
        &ValidationOptions::default().with_level(ValidationLevel::Full),
        None,
    )
    .unwrap();
}
#[test]
fn checksum_failure_is_reported_before_attempting_decompression() {
    let xml = r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:4096:8" compression="zlib:8" checksum="sha1:0000000000000000000000000000000000000000"/></xisf>"#;
    let file = Fixture::new(&xisf(xml, &[0; 8]));
    let error = validate_file(
        &file.0,
        &ValidationOptions::default().with_level(ValidationLevel::Full),
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind(), ValidationErrorKind::IntegrityMismatch);
    assert!(error.to_string().contains("checksum mismatch"));
}
#[test]
fn bad_heap_descriptor_and_invalid_tform_are_rejected() {
    for form in ["1PB(4)", "2PB(4)"] {
        let mut bytes = hdu(&[("SIMPLE", "T"), ("BITPIX", "8"), ("NAXIS", "0")], &[]);
        bytes.extend(hdu(
            &[
                ("XTENSION", "'BINTABLE'"),
                ("BITPIX", "8"),
                ("NAXIS", "2"),
                ("NAXIS1", "8"),
                ("NAXIS2", "1"),
                ("PCOUNT", "4"),
                ("GCOUNT", "1"),
                ("TFIELDS", "1"),
                ("TFORM1", &format!("'{form}'")),
            ],
            &[0, 0, 0, 5, 0, 0, 0, 0, 1, 2, 3, 4],
        ));
        let file = Fixture::new(&bytes);
        assert_eq!(
            validate_file(&file.0, &ValidationOptions::default(), None)
                .unwrap_err()
                .kind(),
            ValidationErrorKind::InvalidStructure
        );
    }
}
#[test]
fn mandatory_fits_card_order_is_checked() {
    let file = Fixture::new(&hdu(
        &[("SIMPLE", "T"), ("NAXIS", "0"), ("BITPIX", "8")],
        &[],
    ));
    assert_eq!(
        validate_file(&file.0, &ValidationOptions::default(), None)
            .unwrap_err()
            .kind(),
        ValidationErrorKind::InvalidStructure
    );
}
#[test]
fn malformed_compression_and_unknown_checksum_are_distinct() {
    for (extra, kind) in [
        (
            "compression=\"zlib:8\" subblocks=\"2,9\"",
            ValidationErrorKind::InvalidStructure,
        ),
        ("checksum=\"future:00\"", ValidationErrorKind::Unsupported),
    ] {
        let xml = format!(
            r#"<?xml version="1.0"?><xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:4096:8" {extra}/></xisf>"#
        );
        let file = Fixture::new(&xisf(&xml, &[0; 8]));
        assert_eq!(
            validate_file(
                &file.0,
                &ValidationOptions::default().with_level(ValidationLevel::Full),
                None
            )
            .unwrap_err()
            .kind(),
            kind
        );
    }
}

#[test]
fn all_standard_sample_widths_and_image_extensions_are_supported() {
    for bitpix in [8i64, 16, 32, 64, -32, -64] {
        let mut b = hdu(&[("SIMPLE", "T"), ("BITPIX", "8"), ("NAXIS", "0")], &[]);
        b.extend(hdu(
            &[
                ("XTENSION", "'IMAGE'"),
                ("BITPIX", &bitpix.to_string()),
                ("NAXIS", "3"),
                ("NAXIS1", "2"),
                ("NAXIS2", "2"),
                ("NAXIS3", "3"),
                ("PCOUNT", "0"),
                ("GCOUNT", "1"),
            ],
            &vec![0; 12 * bitpix.unsigned_abs() as usize / 8],
        ));
        let f = Fixture::new(&b);
        let r = validate_file(
            &f.0,
            &ValidationOptions::default().with_level(ValidationLevel::Full),
            None,
        )
        .unwrap();
        assert_eq!(r.image_count(), 1);
    }
    for (sample, width) in [
        ("UInt8", 1),
        ("UInt16", 2),
        ("UInt32", 4),
        ("UInt64", 8),
        ("Float32", 4),
        ("Float64", 8),
        ("Complex32", 8),
        ("Complex64", 16),
    ] {
        let xml = format!(
            r#"<xisf version="1.0"><Image geometry="2:2:3" sampleFormat="{sample}" location="attachment:4096:{}"/></xisf>"#,
            12 * width
        );
        let f = Fixture::new(&xisf(&xml, &vec![0; 12 * width]));
        validate_file(
            &f.0,
            &ValidationOptions::default().with_level(ValidationLevel::Full),
            None,
        )
        .unwrap();
    }
}
#[test]
fn local_references_are_checked_and_target_blocks_are_validated() {
    for (target, succeeds) in [("profile", true), ("missing", false)] {
        let xml = format!(
            r#"<xisf version="1.0"><Image geometry="1:1:1" sampleFormat="UInt8" location="attachment:4096:1"><Reference ref="{target}"/></Image><ICCProfile uid="profile" location="attachment:4097:4"/></xisf>"#
        );
        let f = Fixture::new(&xisf(&xml, &[0; 5]));
        let result = validate_file(
            &f.0,
            &ValidationOptions::default().with_level(ValidationLevel::Full),
            None,
        );
        if succeeds {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().kind(),
                ValidationErrorKind::InvalidStructure
            );
        }
    }
}
#[test]
fn preallocation_and_absent_checksums_do_not_prove_original_pixel_integrity() {
    let mut bytes = fits();
    bytes[2880..].fill(0);
    let f = Fixture::new(&bytes);
    for level in [ValidationLevel::Structural, ValidationLevel::Full] {
        let report =
            validate_file(&f.0, &ValidationOptions::default().with_level(level), None).unwrap();
        assert_eq!(report.checksums().present(), 0);
        if level == ValidationLevel::Full {
            assert!(report.bytes_read() >= bytes.len() as u64);
        }
    }
}
#[test]
fn binary_tform_optional_suffix_is_legal_and_not_reinterpreted() {
    // FITS 4.0 §7.3.1 defines rTa, with optional a not further specified.
    let mut bytes = hdu(&[("SIMPLE", "T"), ("BITPIX", "8"), ("NAXIS", "0")], &[]);
    bytes.extend(hdu(
        &[
            ("XTENSION", "'BINTABLE'"),
            ("BITPIX", "8"),
            ("NAXIS", "2"),
            ("NAXIS1", "8"),
            ("NAXIS2", "1"),
            ("PCOUNT", "0"),
            ("GCOUNT", "1"),
            ("TFIELDS", "1"),
            ("TFORM1", "'8Boptional'"),
        ],
        &[0; 8],
    ));
    let f = Fixture::new(&bytes);
    validate_file(
        &f.0,
        &ValidationOptions::default().with_level(ValidationLevel::Full),
        None,
    )
    .unwrap();
}
#[test]
fn compressed_corruption_and_unknown_codecs_cannot_be_full_success() {
    for codec in ["zlib", "lz4", "lz4hc", "zstd", "future"] {
        let xml = format!(
            r#"<xisf version="1.0"><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:4096:8" compression="{codec}:8"/></xisf>"#
        );
        let f = Fixture::new(&xisf(&xml, &[0; 8]));
        let report = validate_file(&f.0, &ValidationOptions::default(), None).unwrap();
        assert_eq!(report.undecoded_codecs(), &[codec]);
        let kind = validate_file(
            &f.0,
            &ValidationOptions::default().with_level(ValidationLevel::Full),
            None,
        )
        .unwrap_err()
        .kind();
        assert_eq!(
            kind,
            if codec == "future" {
                ValidationErrorKind::Unsupported
            } else {
                ValidationErrorKind::IntegrityMismatch
            }
        );
    }
}

#[test]
fn embedded_zlib_example_from_xisf_specification_validates() {
    // XISF 1.0 §10.6.3: independent published encoded payload, not our encoder.
    let xml = r#"<xisf version="1.0"><Image geometry="6:6:3" sampleFormat="UInt8" colorSpace="RGB" location="embedded"><Data compression="zlib:108" encoding="base64">eJxjYGBg+A+GEPCfAYkJFQZSUPZ/KBtTBFMXOuc/AwCjKyPd</Data></Image></xisf>"#;
    let f = Fixture::new(&xisf(xml, &[]));
    validate_file(
        &f.0,
        &ValidationOptions::default().with_level(ValidationLevel::Full),
        None,
    )
    .unwrap();
}
#[test]
fn misplaced_data_descriptors_are_not_silently_ignored() {
    for body in [
        r#"<Data encoding="hex" compression="zlib:8">0000</Data>"#,
        r#"<Image geometry="1:1:1" sampleFormat="UInt8" location="embedded" checksum="sha1:00"><Data encoding="hex">00</Data></Image>"#,
    ] {
        let f = Fixture::new(&xisf(&format!(r#"<xisf version="1.0">{body}</xisf>"#), &[]));
        assert_eq!(
            validate_file(
                &f.0,
                &ValidationOptions::default().with_level(ValidationLevel::Full),
                None
            )
            .unwrap_err()
            .kind(),
            ValidationErrorKind::InvalidStructure
        );
    }
}
