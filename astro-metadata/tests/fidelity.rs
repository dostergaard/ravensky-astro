use astro_io::typed::compare::compare_u16_containers;
use astro_io::typed::metadata::append_fits_raw_cards;
use astro_io::typed::write::{
    write_fits_u16, write_xisf_u16_with_metadata, FitsCodec, U16ImageSpec, XisfCodec,
    XisfKeywordSpec,
};
use astro_io::typed::PixelReadLimits;
use astro_io::typed::{inventory, ContainerFormat, InventoryLimits};
use astro_metadata::fidelity::provenance::{
    verify_provenance, PreliminaryVerified, ProvenanceInput, ProvenanceRecord,
};
use astro_metadata::fidelity::{
    compare_metadata, compare_metadata_files, compare_metadata_with_provenance, inspect,
    mapped_fits_cards, mapped_xisf_keywords, Eligibility, ImpactState, LossCategory,
};
use fitsio::{
    tables::{ColumnDataType, ColumnDescription},
    FitsFile,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn path(suffix: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "p2-fidelity-{}-{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            suffix
        )))
    }
    fn xisf(xml: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "p2-fidelity-{}-{}.xisf",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut content = b"XISF0100".to_vec();
        content.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        content.extend_from_slice(&[0; 4]);
        content.extend_from_slice(xml.as_bytes());
        content.resize(4096, 0);
        content.extend_from_slice(bytes);
        fs::write(&path, content).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn per_image_ledger_distinguishes_mappable_keyword_and_six_losses() {
    let xml = concat!(
        "<xisf version=\"1.0\">",
        "<Property id=\"Example:Note\" type=\"String\" value=\"hello\"/>",
        "<Image id=\"LIGHT\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\">",
        "<FITSKeyword name=\"OBJECT\" value=\"'M42'\"/>",
        "<Property id=\"Processing:History\" type=\"String\" value=\"stretch\"/>",
        "<Property id=\"AstrometricSolution:Version\" type=\"String\" value=\"1.0\"/>",
        "<ICCProfile location=\"attachment:4104:4\"/>",
        "<Thumbnail geometry=\"1:1:1\" sampleFormat=\"UInt8\" location=\"attachment:4108:4\"/>",
        "</Image></xisf>"
    );
    let file = Fixture::xisf(xml, &[0; 16]);
    let inv = inventory(&file.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&inv, ContainerFormat::Fits).unwrap();
    assert_eq!(ledger.eligibility, Eligibility::RequiresApproval);
    let losses: Vec<_> = ledger
        .items
        .iter()
        .filter_map(|item| item.category)
        .collect();
    for category in [
        LossCategory::CustomProperty,
        LossCategory::ProvenanceHistory,
        LossCategory::ScientificMetadata,
        LossCategory::ColorProfile,
        LossCategory::DisplayAncillary,
    ] {
        assert!(losses.contains(&category), "missing {category:?}");
    }
    assert!(ledger.items.iter().any(|item| item.identifier == "OBJECT"
        && item.state == ImpactState::ChangedRepresentation
        && item.association == Some(0)));
    assert!(ledger.items.iter().any(|item| item.category
        == Some(LossCategory::ScientificMetadata)
        && item.severity.as_str() == "high"));
}

#[test]
fn unknown_binary_property_blocks_and_same_format_copy_is_distinct() {
    let xml = "<xisf version=\"1.0\"><Image id=\"A\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><Property id=\"Other:Opaque\" type=\"ByteArray\" location=\"attachment:4104:4\" length=\"4\"/></Image></xisf>";
    let file = Fixture::xisf(xml, &[0; 12]);
    let inv = inventory(&file.0, InventoryLimits::default()).unwrap();
    let cross = inspect(&inv, ContainerFormat::Fits).unwrap();
    assert_eq!(cross.eligibility, Eligibility::Blocked);
    assert!(cross
        .items
        .iter()
        .any(|item| item.identifier == "Other:Opaque" && item.state == ImpactState::Unsupported));
    let same = inspect(&inv, ContainerFormat::Xisf).unwrap();
    assert_eq!(same.eligibility, Eligibility::NoKnownLoss);
}

#[test]
fn readback_reports_changed_value_and_missing_association() {
    let source = Fixture::xisf("<xisf version=\"1.0\"><Image id=\"A\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><FITSKeyword name=\"OBJECT\" value=\"'M42'\"/></Image></xisf>", &[0; 8]);
    let changed = Fixture::xisf("<xisf version=\"1.0\"><Image id=\"A\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><FITSKeyword name=\"OBJECT\" value=\"'M43'\"/></Image></xisf>", &[0; 8]);
    let moved = Fixture::xisf("<xisf version=\"1.0\"><FITSKeyword name=\"OBJECT\" value=\"'M42'\"/><Image id=\"A\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"/></xisf>", &[0; 8]);
    let original = inventory(&source.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&original, ContainerFormat::Xisf).unwrap();
    for (file, reason) in [
        (&changed, "value-mismatch"),
        (&moved, "association-mismatch"),
    ] {
        let output = inventory(&file.0, InventoryLimits::default()).unwrap();
        let report = compare_metadata(&original, &output, &ledger).unwrap();
        assert!(!report.passed);
        assert!(report
            .differences
            .iter()
            .any(|difference| difference.reason.as_str() == reason));
    }
}

#[test]
fn binary_property_comparison_requires_bytes_and_detects_same_length_change() {
    let xml = "<xisf version=\"1.0\"><Image id=\"A\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><Property id=\"Other:Opaque\" type=\"ByteArray\" location=\"attachment:4104:4\" length=\"4\"/></Image></xisf>";
    let source = Fixture::xisf(xml, &[0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4]);
    let changed = Fixture::xisf(xml, &[0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 5]);
    let si = inventory(&source.0, InventoryLimits::default()).unwrap();
    let ci = inventory(&changed.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&si, ContainerFormat::Xisf).unwrap();
    assert!(!compare_metadata(&si, &ci, &ledger).unwrap().passed);
    let comparison = compare_metadata_files(&source.0, &si, &changed.0, &ci, &ledger, 2).unwrap();
    assert!(comparison
        .differences
        .iter()
        .any(|difference| difference.reason.as_str() == "value-mismatch"));
    assert!(
        compare_metadata_files(&source.0, &si, &source.0, &si, &ledger, 2)
            .unwrap()
            .passed
    );
}

#[test]
fn native_fitskeyword_projection_survives_both_serialized_directions() {
    let fits_source = Fixture::path("fits");
    let xisf_output = Fixture::path("xisf");
    let fits_recovered = Fixture::path("fits");
    let image = [U16ImageSpec {
        id: None,
        width: 2,
        height: 2,
    }];
    let pixels = [0u16, 1, 32768, 65535];
    let fill = |_: usize, start: u64, out: &mut [u16]| -> anyhow::Result<()> {
        out.copy_from_slice(&pixels[start as usize..start as usize + out.len()]);
        Ok(())
    };
    write_fits_u16(&fits_source.0, &image, FitsCodec::Uncompressed, fill).unwrap();
    let card = |text: &str| format!("{text:<80}");
    append_fits_raw_cards(
        &fits_source.0,
        0,
        &[
            card("OBJECT  = 'M42' / target"),
            card("EXPTIME = 60.0 / seconds"),
            card("CRPIX1  = 1.0"),
        ],
    )
    .unwrap();
    let si = inventory(&fits_source.0, InventoryLimits::default()).unwrap();
    let outbound = inspect(&si, ContainerFormat::Xisf).unwrap();
    assert_eq!(
        outbound.eligibility,
        Eligibility::NoKnownLoss,
        "{:?}",
        outbound.items
    );
    let projected = mapped_xisf_keywords(&si, &outbound).unwrap();
    assert_eq!(
        projected
            .iter()
            .filter(|keyword| keyword.name == "COMMENT")
            .count(),
        0
    );
    assert!(projected.iter().any(|keyword| keyword.name == "OBJECT"));
    let specs: Vec<_> = projected
        .iter()
        .map(|value| XisfKeywordSpec {
            image_index: Some(value.image_index),
            name: &value.name,
            value: &value.value,
            comment: value.comment.as_deref(),
        })
        .collect();
    write_xisf_u16_with_metadata(
        &xisf_output.0,
        &image,
        &[],
        &specs,
        XisfCodec::Uncompressed,
        false,
        fill,
    )
    .unwrap();
    let xi = inventory(&xisf_output.0, InventoryLimits::default()).unwrap();
    let check = compare_metadata(&si, &xi, &outbound).unwrap();
    assert!(check.passed, "{:?}", check.differences);
    assert_eq!(
        compare_u16_containers(
            &fits_source.0,
            &si,
            &xisf_output.0,
            &xi,
            PixelReadLimits::default()
        )
        .unwrap(),
        None
    );

    let inbound = inspect(&xi, ContainerFormat::Fits).unwrap();
    assert_eq!(
        inbound.eligibility,
        Eligibility::NoKnownLoss,
        "{:?}",
        astro_metadata::fidelity::semantic_inventory(&xi)
            .unwrap()
            .records
            .iter()
            .filter(|r| r.identifier == "COMMENT")
            .collect::<Vec<_>>()
    );
    let cards = mapped_fits_cards(&xi, &inbound).unwrap();
    write_fits_u16(&fits_recovered.0, &image, FitsCodec::Uncompressed, fill).unwrap();
    append_fits_raw_cards(
        &fits_recovered.0,
        0,
        &cards
            .iter()
            .map(|card| card.raw_card.clone())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let ri = inventory(&fits_recovered.0, InventoryLimits::default()).unwrap();
    assert!(compare_metadata(&xi, &ri, &inbound).unwrap().passed);
    assert_eq!(
        compare_u16_containers(
            &xisf_output.0,
            &xi,
            &fits_recovered.0,
            &ri,
            PixelReadLimits::default()
        )
        .unwrap(),
        None
    );
}

#[test]
fn provenance_requires_preliminary_readback_and_survives_format_serialization() {
    let image = [U16ImageSpec {
        id: None,
        width: 2,
        height: 2,
    }];
    let source = Fixture::path("fits");
    write_fits_u16(&source.0, &image, FitsCodec::Uncompressed, |_, _, out| {
        out.fill(42);
        Ok(())
    })
    .unwrap();
    let si = inventory(&source.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&si, ContainerFormat::Fits).unwrap();
    let preliminary = compare_metadata(&si, &si, &ledger).unwrap();
    let evidence = PreliminaryVerified::from_readback(None, &preliminary).unwrap();
    let provenance = ProvenanceRecord::from_verified(
        ProvenanceInput {
            application_version: "1.0.0",
            source_format: ContainerFormat::Fits,
            output_format: ContainerFormat::Fits,
            actual_method: "uncompressed",
            operation_id: Some("job-7"),
            approved_losses: &[],
        },
        evidence,
    )
    .unwrap();
    let target = Fixture::path("fits");
    write_fits_u16(&target.0, &image, FitsCodec::Uncompressed, |_, _, out| {
        out.fill(42);
        Ok(())
    })
    .unwrap();
    append_fits_raw_cards(&target.0, 0, &provenance.fits_cards().unwrap()).unwrap();
    let ti = inventory(&target.0, InventoryLimits::default()).unwrap();
    assert!(verify_provenance(&ti, &provenance).passed);
    assert!(!compare_metadata(&si, &ti, &ledger).unwrap().passed);
    assert!(
        compare_metadata_with_provenance(&si, &ti, &ledger, &provenance)
            .unwrap()
            .passed
    );

    let xisf = Fixture::path("xisf");
    let xisf_provenance = ProvenanceRecord::from_verified(
        ProvenanceInput {
            application_version: "1.0.0",
            source_format: ContainerFormat::Fits,
            output_format: ContainerFormat::Xisf,
            actual_method: "zstd",
            operation_id: None,
            approved_losses: &[LossCategory::CustomProperty],
        },
        PreliminaryVerified::from_readback(None, &preliminary).unwrap(),
    )
    .unwrap();
    let owned = xisf_provenance.xisf_properties();
    let specs: Vec<_> = owned
        .iter()
        .map(|(id, value)| astro_io::typed::write::XisfPropertySpec {
            image_index: None,
            id,
            value: astro_io::typed::write::XisfPropertyValue::String(value),
        })
        .collect();
    astro_io::typed::write::write_xisf_u16_with_properties(
        &xisf.0,
        &image,
        &specs,
        XisfCodec::Uncompressed,
        false,
        |_, _, out| {
            out.fill(42);
            Ok(())
        },
    )
    .unwrap();
    let xi = inventory(&xisf.0, InventoryLimits::default()).unwrap();
    assert!(verify_provenance(&xi, &xisf_provenance).passed);
    let outbound = inspect(&si, ContainerFormat::Xisf).unwrap();
    assert!(
        compare_metadata_with_provenance(&si, &xi, &outbound, &xisf_provenance)
            .unwrap()
            .passed
    );
}

#[test]
fn known_notes_table_is_a_named_nonimage_loss_and_unknown_table_blocks() {
    let fixture = Fixture::path("fits");
    let image = [U16ImageSpec {
        id: None,
        width: 2,
        height: 2,
    }];
    write_fits_u16(&fixture.0, &image, FitsCodec::Uncompressed, |_, _, out| {
        out.fill(3);
        Ok(())
    })
    .unwrap();
    astro_io::fits::backend::with_cfitsio(|| {
        let mut file = FitsFile::edit(&fixture.0).unwrap();
        let mut note = ColumnDescription::new("NOTE");
        let note = note
            .with_type(ColumnDataType::String)
            .that_repeats(16)
            .create()
            .unwrap();
        let hdu = file
            .create_table("ASTROMUNINN_NOTES".to_owned(), &[note])
            .unwrap();
        hdu.write_col(&mut file, "NOTE", &["example".to_owned()])
            .unwrap();
        let raw = ColumnDescription::new("RAW")
            .with_type(ColumnDataType::Byte)
            .create()
            .unwrap();
        let unknown = file.create_table("SCI_DATA".to_owned(), &[raw]).unwrap();
        unknown.write_col(&mut file, "RAW", &[1u8, 2, 3]).unwrap();
    });
    let inv = inventory(&fixture.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&inv, ContainerFormat::Xisf).unwrap();
    assert!(ledger
        .items
        .iter()
        .any(|item| item.identifier == "ASTROMUNINN_NOTES"
            && item.category == Some(LossCategory::NonimageExtension)));
    assert!(ledger
        .items
        .iter()
        .any(|item| item.identifier == "SCI_DATA" && item.state == ImpactState::Unsupported));
    assert_eq!(ledger.eligibility, Eligibility::Blocked);
}

#[test]
fn unsupported_auxiliary_image_is_never_an_approvable_metadata_loss() {
    let xml = "<xisf version=\"1.0\"><Image id=\"LIGHT\" geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"/><Image id=\"UNCERTAINTY\" geometry=\"2:2:1\" sampleFormat=\"Float32\" location=\"attachment:4104:16\"/></xisf>";
    let file = Fixture::xisf(xml, &[0; 24]);
    let inv = inventory(&file.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&inv, ContainerFormat::Fits).unwrap();
    assert_eq!(ledger.eligibility, Eligibility::Blocked);
    assert!(ledger.items.iter().any(|item| item.association == Some(1)
        && item.reason.as_str() == "image-capability"
        && item.category.is_none()));
}

#[test]
fn typed_precision_and_wcs_changes_fail_semantic_readback() {
    let source = Fixture::xisf("<xisf version=\"1.0\"><Image geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><FITSKeyword name=\"CRPIX1\" value=\"1.000000000000001\"/><Property id=\"Example:Gain\" type=\"String\" value=\"1.0\"/></Image></xisf>", &[0; 8]);
    let changed = Fixture::xisf("<xisf version=\"1.0\"><Image geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><FITSKeyword name=\"CRPIX1\" value=\"1.0\"/><Property id=\"Example:Gain\" type=\"Float32\" value=\"1.0\"/></Image></xisf>", &[0; 8]);
    let si = inventory(&source.0, InventoryLimits::default()).unwrap();
    let ci = inventory(&changed.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&si, ContainerFormat::Xisf).unwrap();
    let differences = compare_metadata(&si, &ci, &ledger).unwrap().differences;
    assert!(differences
        .iter()
        .any(|difference| difference.identifier == "CRPIX1"
            && difference.reason.as_str() == "value-mismatch"));
    assert!(differences
        .iter()
        .any(|difference| difference.identifier == "Example:Gain"
            && difference.reason.as_str() == "type-mismatch"));
}

#[test]
fn repeated_commentary_is_counted_and_stale_ledgers_are_rejected() {
    let source = Fixture::xisf("<xisf version=\"1.0\"><Image geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><FITSKeyword name=\"COMMENT\" value=\"first\"/><FITSKeyword name=\"COMMENT\" value=\"second\"/><FITSKeyword name=\"HISTORY\" value=\"capture\"/></Image></xisf>", &[0; 8]);
    let extra = Fixture::xisf("<xisf version=\"1.0\"><Image geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><FITSKeyword name=\"COMMENT\" value=\"first\"/><FITSKeyword name=\"COMMENT\" value=\"second\"/><FITSKeyword name=\"HISTORY\" value=\"capture\"/><FITSKeyword name=\"HISTORY\" value=\"unexpected\"/></Image></xisf>", &[0; 8]);
    let si = inventory(&source.0, InventoryLimits::default()).unwrap();
    let ei = inventory(&extra.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&si, ContainerFormat::Xisf).unwrap();
    let report = compare_metadata(&si, &ei, &ledger).unwrap();
    assert!(report.differences.iter().any(|difference| {
        difference.identifier == "HISTORY" && difference.reason.as_str() == "unexpected-property"
    }));
    let mut stale = ledger;
    let item = stale
        .items
        .iter_mut()
        .find(|item| item.identifier == "COMMENT")
        .unwrap();
    item.original_value = Some("other".into());
    assert!(compare_metadata(&si, &si, &stale).is_err());
}

#[test]
fn nina_style_cards_map_per_hdu_and_unknown_keyword_blocks() {
    let file = Fixture::path("fits");
    let images = [
        U16ImageSpec {
            id: None,
            width: 2,
            height: 2,
        },
        U16ImageSpec {
            id: Some("LIGHT"),
            width: 2,
            height: 2,
        },
    ];
    write_fits_u16(&file.0, &images, FitsCodec::Uncompressed, |_, _, out| {
        out.fill(0);
        Ok(())
    })
    .unwrap();
    let card = |text: &str| format!("{text:<80}");
    append_fits_raw_cards(
        &file.0,
        0,
        &[
            card("NINA-VER= '3.0'"),
            card("ZZZ-VER = 'unknown'"),
            card("OBJECT  = 'M42'"),
            card("BAYERPAT= 'RGGB'"),
            card("CRPIX1  = 1.000000000000001"),
        ],
    )
    .unwrap();
    append_fits_raw_cards(
        &file.0,
        1,
        &[card("FILTER  = 'Ha'"), card("HISTORY exposure calibrated")],
    )
    .unwrap();
    let inv = inventory(&file.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&inv, ContainerFormat::Xisf).unwrap();
    let mapped = mapped_xisf_keywords(&inv, &ledger).unwrap();
    assert!(mapped
        .iter()
        .any(|value| value.name == "BAYERPAT" && value.image_index == 0));
    assert!(mapped
        .iter()
        .any(|value| value.name == "FILTER" && value.image_index == 1));
    assert!(mapped
        .iter()
        .any(|value| value.name == "CRPIX1" && value.value == "1.000000000000001"));
    assert!(mapped
        .iter()
        .any(|value| value.name == "HISTORY" && value.image_index == 1));
    assert!(mapped
        .iter()
        .any(|value| value.name == "NINA-VER" && value.image_index == 0));
    assert!(ledger
        .items
        .iter()
        .any(|item| item.identifier == "ZZZ-VER" && item.state == ImpactState::Unsupported));
    assert_eq!(ledger.eligibility, Eligibility::Blocked);
}

#[test]
fn scientific_properties_stay_high_severity_and_foreign_extensions_block() {
    let xml = "<xisf version=\"1.0\" xmlns:other=\"urn:other\"><Image geometry=\"2:2:1\" sampleFormat=\"UInt16\" location=\"attachment:4096:8\"><Property id=\"AstrometricSolution:UnknownTerm\" type=\"Float64\" value=\"1.5\"/></Image><other:Extension/></xisf>";
    let file = Fixture::xisf(xml, &[0; 8]);
    let inv = inventory(&file.0, InventoryLimits::default()).unwrap();
    let ledger = inspect(&inv, ContainerFormat::Fits).unwrap();
    assert!(ledger
        .items
        .iter()
        .any(|item| item.identifier == "AstrometricSolution:UnknownTerm"
            && item.category == Some(LossCategory::ScientificMetadata)
            && item.severity.as_str() == "high"));
    assert!(ledger.items.iter().any(
        |item| item.identifier == "ForeignExtension" && item.state == ImpactState::Unsupported
    ));
    assert_eq!(ledger.eligibility, Eligibility::Blocked);
}
