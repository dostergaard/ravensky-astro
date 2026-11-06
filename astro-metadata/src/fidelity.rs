//! Per-image metadata impact and semantic readback for the restricted P1
//! transformation subset. Raw syntax remains in `astro-io`; this module
//! classifies meaning and writer compatibility without granting approvals.

pub mod provenance;

use self::provenance::{verify_provenance, ProvenanceRecord};
use anyhow::{bail, Result};
use astro_io::typed::{
    read_ancillary_chunk, AncillaryReference, ContainerFormat, ContainerInventory,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

const MAX_RECORDS: usize = 1_100_000;
const MAX_BINARY_PROPERTY_BYTES: u64 = 16 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LossCategory {
    CustomProperty,
    ProvenanceHistory,
    DisplayAncillary,
    ColorProfile,
    NonimageExtension,
    ScientificMetadata,
}

impl LossCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CustomProperty => "custom-property",
            Self::ProvenanceHistory => "provenance-history",
            Self::DisplayAncillary => "display-ancillary",
            Self::ColorProfile => "color-profile",
            Self::NonimageExtension => "nonimage-extension",
            Self::ScientificMetadata => "scientific-metadata",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    High,
}
impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImpactState {
    Unchanged,
    ChangedRepresentation,
    ChangedMeaning,
    Lost,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Eligibility {
    NoKnownLoss,
    RequiresApproval,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasonCode {
    NativeEquivalent,
    StructuralRegenerated,
    KnownLoss,
    UnknownSemantics,
    MissingWriter,
    ImageCapability,
    UnmappedScientific,
    UnmappedAncillary,
    UnsupportedAssociation,
    UnsupportedValue,
}
impl ReasonCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NativeEquivalent => "native-equivalent",
            Self::StructuralRegenerated => "structural-regenerated",
            Self::KnownLoss => "known-loss",
            Self::UnknownSemantics => "unknown-semantics",
            Self::MissingWriter => "missing-writer",
            Self::ImageCapability => "image-capability",
            Self::UnmappedScientific => "unmapped-scientific",
            Self::UnmappedAncillary => "unmapped-ancillary",
            Self::UnsupportedAssociation => "unsupported-association",
            Self::UnsupportedValue => "unsupported-value",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticRole {
    Structural,
    Acquisition,
    Wcs,
    Cfa,
    Calibration,
    Provenance,
    Display,
    Color,
    Custom,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "format", rename_all = "kebab-case")]
pub enum SourceRef {
    FitsCard { hdu: usize, card: usize },
    FitsHdu { hdu: usize },
    XisfElement { element: usize },
    Image { image: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordKind {
    FitsCard,
    FitsHdu,
    FitsKeyword,
    Property,
    Ancillary,
    Core,
}

/// A typed source fact with its stable raw-inventory reference. `text` is the
/// original decoded lexical value; binary data are referenced by length, never
/// silently treated as an empty scalar or loaded during ledger construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticRecord {
    pub source: SourceRef,
    pub kind: RecordKind,
    pub association: Option<usize>,
    pub identifier: String,
    pub declared_type: String,
    pub text: Option<String>,
    pub comment: Option<String>,
    pub binary_length: Option<u64>,
    pub encoding: Option<String>,
    pub location: Option<String>,
    pub role: SemanticRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticImage {
    pub index: usize,
    pub source_index: usize,
    pub id: Option<String>,
    pub axes: Vec<u64>,
    pub sample_format: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticInventory {
    pub format: ContainerFormat,
    pub images: Vec<SemanticImage>,
    pub records: Vec<SemanticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImpactItem {
    pub source: SourceRef,
    pub association: Option<usize>,
    pub identifier: String,
    pub original_type: String,
    pub original_value: Option<String>,
    pub original_comment: Option<String>,
    pub original_binary_length: Option<u64>,
    pub destination_representation: Option<String>,
    pub state: ImpactState,
    pub reason: ReasonCode,
    pub category: Option<LossCategory>,
    pub severity: Severity,
    pub reversible_native: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImpactLedger {
    pub source_format: ContainerFormat,
    pub destination_format: ContainerFormat,
    pub images: Vec<SemanticImage>,
    pub items: Vec<ImpactItem>,
    pub eligibility: Eligibility,
}

fn role_for_fits(name: &str) -> SemanticRole {
    if matches!(
        name,
        "COMMENT" | "HISTORY" | "CREATOR" | "SWCREATE" | "SOFTWARE" | "NINA-VER" | "EKOS-VER"
    ) {
        return SemanticRole::Provenance;
    }
    if matches!(name, "BAYERPAT" | "COLORTYP" | "CFAOFFX" | "CFAOFFY") {
        return SemanticRole::Cfa;
    }
    if matches!(
        name,
        "BUNIT" | "PEDESTAL" | "DARKTIME" | "CALSTAT" | "BIASSEC" | "TRIMSEC"
    ) {
        return SemanticRole::Calibration;
    }
    if matches!(name, "EQUINOX" | "RADESYS" | "WCSAXES")
        || [
            "CRPIX", "CRVAL", "CTYPE", "CUNIT", "CDELT", "CROTA", "CD", "PC", "PV", "PS", "A_",
            "B_", "AP_", "BP_",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return SemanticRole::Wcs;
    }
    if matches!(
        name,
        "OBJECT"
            | "FILTER"
            | "FILTERID"
            | "FLTPOS"
            | "DATE-OBS"
            | "EXPTIME"
            | "EXPOSURE"
            | "TELESCOP"
            | "INSTRUME"
            | "GAIN"
            | "EGAIN"
            | "XBINNING"
            | "YBINNING"
            | "IMAGETYP"
            | "FRAME"
            | "RA"
            | "DEC"
            | "OBJCTRA"
            | "OBJCTDEC"
            | "FOCALLEN"
            | "XPIXSZ"
            | "YPIXSZ"
            | "PIXSIZE"
            | "CCD-TEMP"
            | "CCDTEMP"
            | "SET-TEMP"
            | "COOLPWR"
            | "READOUT"
            | "READOUTM"
            | "RDNOISE"
            | "APERTURE"
            | "ROTANG"
            | "ROTPA"
            | "FRAMENUM"
            | "SEQNUM"
            | "SEQID"
    ) {
        return SemanticRole::Acquisition;
    }
    SemanticRole::Unknown
}

fn structural_fits(name: &str) -> bool {
    matches!(
        name,
        "SIMPLE"
            | "XTENSION"
            | "BITPIX"
            | "BSCALE"
            | "BZERO"
            | "BLANK"
            | "PCOUNT"
            | "GCOUNT"
            | "TFIELDS"
            | "EXTEND"
            | "END"
            | "CHECKSUM"
            | "DATASUM"
            | "ZIMAGE"
            | "ZBITPIX"
            | "ZCMPTYPE"
            | "ZBZERO"
            | "ZBSCALE"
            | "ZBLANK"
            | "EXTNAME"
            | "EXTVER"
    ) || [
        "NAXIS", "ZNAXIS", "ZTILE", "ZNAME", "ZVAL", "TTYPE", "TFORM", "TUNIT", "TDIM",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

// CFITSIO inserts these bibliographic cards when it creates a primary HDU.
// They are regenerated by the P1 FITS writer and must not be projected twice.
fn generated_reference_comment(name: &str, value: Option<&str>) -> bool {
    name == "COMMENT"
        && matches!(
            value.map(str::trim_start),
            Some("FITS (Flexible Image Transport System) format is defined in 'Astronomy")
                | Some("and Astrophysics', volume 376, page 359; bibcode: 2001A&A...376..359H")
        )
}

fn role_for_xisf(name: &str, id: &str) -> SemanticRole {
    match name {
        "ICCProfile" => SemanticRole::Color,
        "Thumbnail" | "DisplayFunction" | "RGBWorkingSpace" => SemanticRole::Display,
        "ColorFilterArray" => SemanticRole::Cfa,
        "Resolution" => SemanticRole::Calibration,
        "FITSKeyword" => role_for_fits(id),
        "Property" if id.starts_with("AstrometricSolution:") => SemanticRole::Wcs,
        "Property"
            if id.starts_with("Processing:")
                || id.starts_with("XISF:Creator")
                || id == "XISF:CreationTime" =>
        {
            SemanticRole::Provenance
        }
        "Property"
            if id.starts_with("Observation:")
                || id.starts_with("Instrument:")
                || id.starts_with("Image:") =>
        {
            SemanticRole::Acquisition
        }
        "Property" if id.starts_with("XISF:") => SemanticRole::Structural,
        "Property" if id.contains(':') => SemanticRole::Custom,
        _ => SemanticRole::Unknown,
    }
}

/// Project every admitted P1 raw record. This does not change the legacy
/// first-image `AstroMetadata` extractor or infer equivalence between keys
/// whose producer-specific meaning is unknown.
pub fn semantic_inventory(input: &ContainerInventory) -> Result<SemanticInventory> {
    let images = input
        .images
        .iter()
        .enumerate()
        .map(|(index, image)| SemanticImage {
            index,
            source_index: image.source_index,
            id: image.id.clone(),
            axes: image.axes.clone(),
            sample_format: image.sample_format.clone(),
        })
        .collect();
    let mut records = Vec::new();
    match input.format {
        ContainerFormat::Fits => {
            for hdu in &input.fits_hdus {
                if hdu.image_index.is_none() && hdu.index > 0 {
                    let notes = hdu.kind == "table"
                        && hdu.cards.iter().any(|card| {
                            card.keyword == "EXTNAME"
                                && card.value.as_deref() == Some("ASTROMUNINN_NOTES")
                        })
                        && hdu.cards.iter().any(|card| {
                            card.keyword == "TFIELDS" && card.value.as_deref() == Some("1")
                        })
                        && hdu.cards.iter().any(|card| {
                            card.keyword == "TTYPE1" && card.value.as_deref() == Some("NOTE")
                        })
                        && hdu.cards.iter().any(|card| {
                            card.keyword == "TFORM1"
                                && card
                                    .value
                                    .as_deref()
                                    .is_some_and(|value| value.ends_with('A'))
                        });
                    records.push(SemanticRecord {
                        source: SourceRef::FitsHdu { hdu: hdu.index },
                        kind: RecordKind::FitsHdu,
                        association: None,
                        identifier: hdu
                            .cards
                            .iter()
                            .rev()
                            .find(|card| card.keyword == "EXTNAME")
                            .and_then(|card| card.value.clone())
                            .unwrap_or_else(|| format!("HDU{}", hdu.index)),
                        declared_type: if notes {
                            "notes-table".into()
                        } else {
                            hdu.kind.clone()
                        },
                        text: None,
                        comment: None,
                        binary_length: Some(hdu.data.length),
                        encoding: None,
                        location: None,
                        role: SemanticRole::Unknown,
                    });
                }
                for card in &hdu.cards {
                    records.push(SemanticRecord {
                        source: SourceRef::FitsCard {
                            hdu: hdu.index,
                            card: card.card_index,
                        },
                        kind: RecordKind::FitsCard,
                        association: hdu.image_index,
                        identifier: card.keyword.clone(),
                        declared_type: fits_type(card.raw_card.as_deref(), card.value.as_deref())
                            .into(),
                        text: card.value.clone().or_else(|| {
                            matches!(card.keyword.as_str(), "HISTORY" | "COMMENT").then(|| {
                                card.raw_card
                                    .as_deref()
                                    .unwrap_or("")
                                    .get(8..)
                                    .unwrap_or("")
                                    .trim_end()
                                    .to_owned()
                            })
                        }),
                        comment: if matches!(card.keyword.as_str(), "HISTORY" | "COMMENT") {
                            None
                        } else {
                            card.comment.clone()
                        },
                        binary_length: None,
                        encoding: None,
                        location: None,
                        role: if structural_fits(&card.keyword)
                            || generated_reference_comment(
                                &card.keyword,
                                card.value.as_deref().or_else(|| {
                                    card.raw_card.as_deref().and_then(|raw| raw.get(8..))
                                }),
                            ) {
                            SemanticRole::Structural
                        } else {
                            role_for_fits(&card.keyword)
                        },
                    });
                }
            }
        }
        ContainerFormat::Xisf => {
            for element in &input.xisf_elements {
                if matches!(element.name.as_str(), "xisf" | "Image" | "Metadata") {
                    continue;
                }
                let id = element
                    .attributes
                    .get(if element.name == "FITSKeyword" {
                        "name"
                    } else {
                        "id"
                    })
                    .cloned()
                    .unwrap_or_else(|| element.name.clone());
                let kind = match element.name.as_str() {
                    "Property" => RecordKind::Property,
                    "FITSKeyword" => RecordKind::FitsKeyword,
                    "ICCProfile" | "Thumbnail" | "DisplayFunction" | "RGBWorkingSpace"
                    | "ColorFilterArray" | "Resolution" => RecordKind::Ancillary,
                    _ => RecordKind::Core,
                };
                let text = element
                    .attributes
                    .get("value")
                    .cloned()
                    .or_else(|| (!element.text.is_empty()).then(|| element.text.clone()));
                let declared_type = if element.name == "FITSKeyword" {
                    let raw = text.as_deref().unwrap_or("");
                    if matches!(id.as_str(), "COMMENT" | "HISTORY") {
                        "Text".to_owned()
                    } else if raw.starts_with('\'') && raw.ends_with('\'') {
                        "String".to_owned()
                    } else if matches!(raw, "T" | "F") {
                        "Boolean".to_owned()
                    } else if decimal_key(raw).is_some() {
                        "Number".to_owned()
                    } else {
                        "Opaque".to_owned()
                    }
                } else {
                    element
                        .attributes
                        .get("type")
                        .cloned()
                        .unwrap_or_else(|| element.name.clone())
                };
                records.push(SemanticRecord {
                    source: SourceRef::XisfElement {
                        element: element.index,
                    },
                    kind,
                    association: element.parent_image_index,
                    identifier: id.clone(),
                    declared_type,
                    text,
                    comment: element.attributes.get("comment").cloned(),
                    binary_length: element.attachment.map(|extent| extent.length),
                    encoding: element.attributes.get("compression").cloned(),
                    location: element.attributes.get("location").cloned(),
                    role: if element.name == "FITSKeyword"
                        && generated_reference_comment(
                            &id,
                            element.attributes.get("value").map(String::as_str),
                        ) {
                        SemanticRole::Structural
                    } else {
                        role_for_xisf(&element.name, &id)
                    },
                });
            }
        }
    }
    if records.len() > MAX_RECORDS {
        bail!("Metadata record count exceeds P2 limit");
    }
    Ok(SemanticInventory {
        format: input.format,
        images,
        records,
    })
}

fn fits_type(raw: Option<&str>, value: Option<&str>) -> &'static str {
    if raw.is_some_and(|card| {
        card.get(10..)
            .is_some_and(|tail| tail.trim_start().starts_with('\''))
    }) {
        "String"
    } else if matches!(value, Some("T" | "F")) {
        "Boolean"
    } else if value.is_some_and(|text| decimal_key(text).is_some()) {
        "Number"
    } else {
        "Text"
    }
}

fn native_fits_keyword(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 8
        && name.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        && !structural_fits(name)
        && role_for_fits(name) != SemanticRole::Unknown
}

fn loss_category(role: SemanticRole) -> Option<LossCategory> {
    match role {
        SemanticRole::Wcs
        | SemanticRole::Cfa
        | SemanticRole::Calibration
        | SemanticRole::Acquisition => Some(LossCategory::ScientificMetadata),
        SemanticRole::Provenance => Some(LossCategory::ProvenanceHistory),
        SemanticRole::Display => Some(LossCategory::DisplayAncillary),
        SemanticRole::Color => Some(LossCategory::ColorProfile),
        SemanticRole::Custom => Some(LossCategory::CustomProperty),
        SemanticRole::Structural | SemanticRole::Unknown => None,
    }
}

fn item(
    record: &SemanticRecord,
    state: ImpactState,
    reason: ReasonCode,
    destination: Option<&str>,
    category: Option<LossCategory>,
) -> ImpactItem {
    let severity = if category == Some(LossCategory::ScientificMetadata) {
        Severity::High
    } else if category.is_some() || state == ImpactState::Unsupported {
        Severity::Warning
    } else {
        Severity::Info
    };
    ImpactItem {
        source: record.source,
        association: record.association,
        identifier: record.identifier.clone(),
        original_type: record.declared_type.clone(),
        original_value: record.text.clone(),
        original_comment: record.comment.clone(),
        original_binary_length: record.binary_length,
        destination_representation: destination.map(str::to_owned),
        state,
        reason,
        category,
        severity,
        reversible_native: destination.is_some()
            && category.is_none()
            && state != ImpactState::Unsupported,
        description: format!("{}: {}", reason.as_str(), record.identifier),
    }
}

/// Classify metadata for a proposed *rewrite*. No loss is authorized here.
/// `RequiresApproval` is only a fact for P3; `Blocked` always overrides it.
pub fn inspect(input: &ContainerInventory, destination: ContainerFormat) -> Result<ImpactLedger> {
    let source = semantic_inventory(input)?;
    let mut items = Vec::new();
    if source.images.is_empty() {
        items.push(ImpactItem {
            source: SourceRef::Image { image: 0 },
            association: None,
            identifier: "images".into(),
            original_type: "none".into(),
            original_value: None,
            original_comment: None,
            original_binary_length: None,
            destination_representation: None,
            state: ImpactState::Unsupported,
            reason: ReasonCode::ImageCapability,
            category: None,
            severity: Severity::High,
            reversible_native: false,
            description: "No image array is available for the P1 writer".into(),
        });
    }
    for image in &source.images {
        let shape = match source.format {
            ContainerFormat::Fits => image.axes.as_slice(),
            ContainerFormat::Xisf if image.axes.len() == 3 && image.axes[2] == 1 => {
                &image.axes[..2]
            }
            _ => &[],
        };
        if image.sample_format != "UInt16" || shape.len() != 2 || shape.contains(&0) {
            items.push(ImpactItem { source: SourceRef::Image { image: image.index }, association: Some(image.index), identifier: image.id.clone().unwrap_or_else(|| format!("image:{}", image.index)), original_type: image.sample_format.clone(), original_value: None, original_comment: None, original_binary_length: None, destination_representation: None, state: ImpactState::Unsupported, reason: ReasonCode::ImageCapability, category: None, severity: Severity::High, reversible_native: false, description: "Image array lacks an exact P1 writer/reader capability; metadata approval cannot authorize its loss".into() });
        }
    }
    for record in &source.records {
        let entry = match record.kind {
            RecordKind::FitsHdu => {
                if record.identifier == "ASTROMUNINN_NOTES" && record.declared_type == "notes-table"
                {
                    item(
                        record,
                        ImpactState::Lost,
                        ReasonCode::KnownLoss,
                        None,
                        Some(LossCategory::NonimageExtension),
                    )
                } else {
                    item(
                        record,
                        ImpactState::Unsupported,
                        ReasonCode::UnknownSemantics,
                        None,
                        None,
                    )
                }
            }
            RecordKind::FitsCard => classify_fits_card(record, destination),
            RecordKind::FitsKeyword => classify_xisf_keyword(record, destination),
            RecordKind::Property => classify_property(record, destination),
            RecordKind::Ancillary => {
                if let Some(category) = loss_category(record.role) {
                    item(
                        record,
                        ImpactState::Lost,
                        ReasonCode::UnmappedAncillary,
                        None,
                        Some(category),
                    )
                } else {
                    item(
                        record,
                        ImpactState::Unsupported,
                        ReasonCode::UnknownSemantics,
                        None,
                        None,
                    )
                }
            }
            RecordKind::Core => item(
                record,
                ImpactState::Unsupported,
                ReasonCode::UnknownSemantics,
                None,
                None,
            ),
        };
        items.push(entry);
    }
    let eligibility = if items
        .iter()
        .any(|entry| entry.state == ImpactState::Unsupported)
    {
        Eligibility::Blocked
    } else if items.iter().any(|entry| entry.category.is_some()) {
        Eligibility::RequiresApproval
    } else {
        Eligibility::NoKnownLoss
    };
    Ok(ImpactLedger {
        source_format: source.format,
        destination_format: destination,
        images: source.images,
        items,
        eligibility,
    })
}

fn classify_fits_card(record: &SemanticRecord, target: ContainerFormat) -> ImpactItem {
    if record.role == SemanticRole::Structural {
        return item(
            record,
            ImpactState::Unchanged,
            ReasonCode::StructuralRegenerated,
            Some("generated structural card"),
            None,
        );
    }
    if target == ContainerFormat::Fits {
        return item(
            record,
            ImpactState::Unchanged,
            ReasonCode::NativeEquivalent,
            Some("FITS card"),
            None,
        );
    }
    if native_fits_keyword(&record.identifier)
        && projected_fits_literal(record).is_some()
        && record.comment.as_deref().is_none_or(|comment| {
            comment.len() <= 70 && comment.bytes().all(|byte| (32..=126).contains(&byte))
        })
    {
        return item(
            record,
            ImpactState::ChangedRepresentation,
            ReasonCode::NativeEquivalent,
            Some("XISF FITSKeyword"),
            None,
        );
    }
    if let Some(category) = loss_category(record.role) {
        return item(
            record,
            ImpactState::Lost,
            ReasonCode::KnownLoss,
            None,
            Some(category),
        );
    }
    item(
        record,
        ImpactState::Unsupported,
        ReasonCode::UnknownSemantics,
        None,
        None,
    )
}

fn classify_xisf_keyword(record: &SemanticRecord, target: ContainerFormat) -> ImpactItem {
    if record.role == SemanticRole::Structural {
        return item(
            record,
            ImpactState::Unchanged,
            ReasonCode::StructuralRegenerated,
            Some("generated FITS reference comment"),
            None,
        );
    }
    if target == ContainerFormat::Xisf {
        return item(
            record,
            ImpactState::Unchanged,
            ReasonCode::NativeEquivalent,
            Some("XISF FITSKeyword"),
            None,
        );
    }
    if record.association.is_none() {
        return item(
            record,
            ImpactState::Unsupported,
            ReasonCode::UnsupportedAssociation,
            None,
            None,
        );
    }
    if native_fits_keyword(&record.identifier)
        && record.text.as_deref().is_some_and(|value| {
            fits_card_for_keyword(&record.identifier, value, record.comment.as_deref()).is_ok()
        })
    {
        return item(
            record,
            ImpactState::ChangedRepresentation,
            ReasonCode::NativeEquivalent,
            Some("FITS card"),
            None,
        );
    }
    if let Some(category) = loss_category(record.role) {
        return item(
            record,
            ImpactState::Lost,
            ReasonCode::KnownLoss,
            None,
            Some(category),
        );
    }
    item(
        record,
        ImpactState::Unsupported,
        ReasonCode::UnknownSemantics,
        None,
        None,
    )
}

fn classify_property(record: &SemanticRecord, target: ContainerFormat) -> ImpactItem {
    if record
        .location
        .as_deref()
        .is_some_and(|location| !location.starts_with("attachment:"))
        || record.encoding.is_some()
        || record
            .binary_length
            .is_some_and(|length| length > MAX_BINARY_PROPERTY_BYTES)
    {
        return item(
            record,
            ImpactState::Unsupported,
            ReasonCode::MissingWriter,
            None,
            None,
        );
    }
    if record.declared_type == "ByteArray" && record.binary_length.is_none() {
        return item(
            record,
            ImpactState::Unsupported,
            ReasonCode::MissingWriter,
            None,
            None,
        );
    }
    if target == ContainerFormat::Xisf
        && matches!(record.declared_type.as_str(), "String" | "ByteArray")
    {
        return item(
            record,
            ImpactState::Unchanged,
            ReasonCode::NativeEquivalent,
            Some("XISF Property"),
            None,
        );
    }
    if record.binary_length.is_some() && record.role == SemanticRole::Custom {
        return item(
            record,
            ImpactState::Unsupported,
            ReasonCode::UnknownSemantics,
            None,
            None,
        );
    }
    if record.text.is_none() && record.binary_length.is_none() {
        return item(
            record,
            ImpactState::Unsupported,
            ReasonCode::UnsupportedValue,
            None,
            None,
        );
    }
    if let Some(category) = loss_category(record.role) {
        return item(
            record,
            ImpactState::Lost,
            ReasonCode::KnownLoss,
            None,
            Some(category),
        );
    }
    item(
        record,
        ImpactState::Unsupported,
        ReasonCode::UnknownSemantics,
        None,
        None,
    )
}

fn valid_fits_literal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 70
        && value.bytes().all(|byte| (32..=126).contains(&byte))
        && (value.starts_with('\'') && value.ends_with('\'')
            || decimal_key(value).is_some()
            || matches!(value, "T" | "F"))
}

fn projected_fits_literal(record: &SemanticRecord) -> Option<String> {
    let value = record.text.as_deref()?;
    let literal = match record.declared_type.as_str() {
        "String" => format!("'{}'", value.replace('\'', "''")),
        "Number" if decimal_key(value).is_some() => value.to_owned(),
        "Boolean" if matches!(value, "T" | "F") => value.to_owned(),
        "Text" if matches!(record.identifier.as_str(), "COMMENT" | "HISTORY") => {
            value.trim_start().to_owned()
        }
        _ => return None,
    };
    if matches!(record.identifier.as_str(), "COMMENT" | "HISTORY") {
        (!literal.is_empty()
            && literal.len() <= 70
            && literal.bytes().all(|byte| (32..=126).contains(&byte)))
        .then_some(literal)
    } else {
        valid_fits_literal(&literal).then_some(literal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectedXisfKeyword {
    pub image_index: usize,
    pub name: String,
    pub value: String,
    pub comment: Option<String>,
    pub source: SourceRef,
}

/// Extract only ledger-approved *native mapping facts* for the P1 XISF
/// FITSKeyword writer. No loss approvals are inferred from this projection.
pub fn mapped_xisf_keywords(
    input: &ContainerInventory,
    ledger: &ImpactLedger,
) -> Result<Vec<ProjectedXisfKeyword>> {
    if input.format != ContainerFormat::Fits
        || ledger.source_format != ContainerFormat::Fits
        || ledger.destination_format != ContainerFormat::Xisf
    {
        bail!("FITS to XISF ledger required");
    }
    let facts = semantic_inventory(input)?;
    let impacts: BTreeMap<_, _> = ledger
        .items
        .iter()
        .map(|item| (item.source, item))
        .collect();
    let mut output = Vec::new();
    for record in &facts.records {
        if !matches!(record.kind, RecordKind::FitsCard)
            || record.role == SemanticRole::Structural
            || !native_fits_keyword(&record.identifier)
        {
            continue;
        }
        let impact = impacts
            .get(&record.source)
            .ok_or_else(|| anyhow::anyhow!("Missing source impact"))?;
        validate_impact(record, impact)?;
        if impact.state != ImpactState::ChangedRepresentation {
            continue;
        }
        let image_index = record
            .association
            .ok_or_else(|| anyhow::anyhow!("Unassociated FITS keyword cannot be projected"))?;
        output.push(ProjectedXisfKeyword {
            image_index,
            name: record.identifier.clone(),
            value: projected_fits_literal(record)
                .ok_or_else(|| anyhow::anyhow!("Unsupported FITS value"))?,
            comment: record.comment.clone(),
            source: record.source,
        });
    }
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectedFitsCard {
    pub image_index: usize,
    pub raw_card: String,
    pub source: SourceRef,
}

/// Extract the supported native FITS card representation of XISF FITSKeyword
/// elements. The caller maps image index to output HDU and uses P1's checked
/// raw-card appender; omitted records remain visible in the ledger.
pub fn mapped_fits_cards(
    input: &ContainerInventory,
    ledger: &ImpactLedger,
) -> Result<Vec<ProjectedFitsCard>> {
    if input.format != ContainerFormat::Xisf
        || ledger.source_format != ContainerFormat::Xisf
        || ledger.destination_format != ContainerFormat::Fits
    {
        bail!("XISF to FITS ledger required");
    }
    let facts = semantic_inventory(input)?;
    let impacts: BTreeMap<_, _> = ledger
        .items
        .iter()
        .map(|item| (item.source, item))
        .collect();
    let mut output = Vec::new();
    for record in &facts.records {
        if record.kind != RecordKind::FitsKeyword || record.role == SemanticRole::Structural {
            continue;
        }
        let impact = impacts
            .get(&record.source)
            .ok_or_else(|| anyhow::anyhow!("Missing source impact"))?;
        validate_impact(record, impact)?;
        if impact.state != ImpactState::ChangedRepresentation {
            continue;
        }
        let image_index = record
            .association
            .ok_or_else(|| anyhow::anyhow!("Unassociated FITSKeyword cannot be projected"))?;
        output.push(ProjectedFitsCard {
            image_index,
            raw_card: fits_card_for_keyword(
                &record.identifier,
                record
                    .text
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Missing FITSKeyword value"))?,
                record.comment.as_deref(),
            )?,
            source: record.source,
        });
    }
    Ok(output)
}

/// Construct a strict 80-byte FITS card for a supported XISF FITSKeyword.
/// Caller must append only after `inspect` has selected a native mapping.
pub fn fits_card_for_keyword(name: &str, value: &str, comment: Option<&str>) -> Result<String> {
    if !native_fits_keyword(name) {
        bail!("Unsupported FITSKeyword mapping");
    }
    if matches!(name, "COMMENT" | "HISTORY") {
        if comment.is_some()
            || value.is_empty()
            || value.len() > 70
            || !value.bytes().all(|byte| (32..=126).contains(&byte))
        {
            bail!("Unsupported FITS commentary card");
        }
        let mut card = format!("{name:<8}{value}");
        card.extend(std::iter::repeat_n(' ', 80 - card.len()));
        return Ok(card);
    }
    if !valid_fits_literal(value) {
        bail!("Unsupported FITSKeyword mapping");
    }
    if comment.is_some_and(|text| {
        !text.is_ascii() || text.bytes().any(|byte| !(32..=126).contains(&byte))
    }) {
        bail!("Unsupported FITS comment");
    }
    let mut card = format!("{name:<8}= {value}");
    if let Some(comment) = comment {
        card.push_str(" / ");
        card.push_str(comment);
    }
    if card.len() > 80 {
        bail!("FITS card exceeds 80 bytes");
    }
    card.extend(std::iter::repeat_n(' ', 80 - card.len()));
    Ok(card)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComparisonReason {
    MissingProperty,
    UnexpectedProperty,
    ValueMismatch,
    TypeMismatch,
    AssociationMismatch,
    UnsupportedComparison,
    ImageMismatch,
}
impl ComparisonReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingProperty => "missing-property",
            Self::UnexpectedProperty => "unexpected-property",
            Self::ValueMismatch => "value-mismatch",
            Self::TypeMismatch => "type-mismatch",
            Self::AssociationMismatch => "association-mismatch",
            Self::UnsupportedComparison => "unsupported-comparison",
            Self::ImageMismatch => "image-mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetadataDifference {
    pub source: SourceRef,
    pub identifier: String,
    pub reason: ComparisonReason,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComparisonReport {
    pub passed: bool,
    pub differences: Vec<MetadataDifference>,
}

/// Compare source facts with independently inventoried output bytes. Expected
/// losses are skipped only when they are explicitly in the ledger; approval is
/// still P3's job. This never compares pixels or asserts output-file validity.
pub fn compare_metadata(
    source: &ContainerInventory,
    output: &ContainerInventory,
    ledger: &ImpactLedger,
) -> Result<ComparisonReport> {
    compare_metadata_inner(source, output, ledger, None, None)
}

/// Compare a finalized output that also contains the exact provenance fields
/// written after preliminary verification. Only the supplied record's fields
/// are accepted as additions; other new metadata still fails comparison.
pub fn compare_metadata_with_provenance(
    source: &ContainerInventory,
    output: &ContainerInventory,
    ledger: &ImpactLedger,
    provenance: &ProvenanceRecord,
) -> Result<ComparisonReport> {
    compare_metadata_inner(source, output, ledger, None, Some(provenance))
}

/// As above, with exact bounded comparison of native XISF ByteArray payloads.
/// The path-free variant reports binary comparisons as unsupported.
pub fn compare_metadata_files(
    source_path: &Path,
    source: &ContainerInventory,
    output_path: &Path,
    output: &ContainerInventory,
    ledger: &ImpactLedger,
    max_chunk_bytes: usize,
) -> Result<ComparisonReport> {
    if max_chunk_bytes == 0 || max_chunk_bytes > 1 << 20 {
        bail!("Metadata comparison chunk limit must be within 1..=1 MiB");
    }
    compare_metadata_inner(
        source,
        output,
        ledger,
        Some((source_path, output_path, max_chunk_bytes)),
        None,
    )
}

/// File-backed variant for finalized outputs with preserved binary properties.
pub fn compare_metadata_files_with_provenance(
    source_path: &Path,
    source: &ContainerInventory,
    output_path: &Path,
    output: &ContainerInventory,
    ledger: &ImpactLedger,
    max_chunk_bytes: usize,
    provenance: &ProvenanceRecord,
) -> Result<ComparisonReport> {
    if max_chunk_bytes == 0 || max_chunk_bytes > 1 << 20 {
        bail!("Metadata comparison chunk limit must be within 1..=1 MiB");
    }
    compare_metadata_inner(
        source,
        output,
        ledger,
        Some((source_path, output_path, max_chunk_bytes)),
        Some(provenance),
    )
}

fn compare_metadata_inner(
    source: &ContainerInventory,
    output: &ContainerInventory,
    ledger: &ImpactLedger,
    paths: Option<(&Path, &Path, usize)>,
    provenance: Option<&ProvenanceRecord>,
) -> Result<ComparisonReport> {
    if source.format != ledger.source_format || output.format != ledger.destination_format {
        bail!("Metadata comparison format differs from ledger");
    }
    let source_facts = semantic_inventory(source)?;
    let output_facts = semantic_inventory(output)?;
    if source_facts.images != ledger.images {
        bail!("Ledger image facts differ from source inventory");
    }
    if source_facts.records.len()
        != ledger
            .items
            .iter()
            .filter(|item| !matches!(item.source, SourceRef::Image { .. }))
            .count()
    {
        bail!("Ledger does not match source metadata inventory");
    }
    let mut differences = Vec::new();
    if source_facts.images.len() != output_facts.images.len() {
        differences.push(MetadataDifference {
            source: SourceRef::Image { image: 0 },
            identifier: "images".into(),
            reason: ComparisonReason::ImageMismatch,
            description: "Image count differs; exact P1 pixel comparison is also required".into(),
        });
    }
    for (left, right) in source_facts.images.iter().zip(&output_facts.images) {
        if canonical_image_id(source.format, left) != canonical_image_id(output.format, right)
            || canonical_axes(source.format, &left.axes)
                != canonical_axes(output.format, &right.axes)
            || left.sample_format != right.sample_format
        {
            differences.push(MetadataDifference {
                source: SourceRef::Image { image: left.index },
                identifier: left.id.clone().unwrap_or_default(),
                reason: ComparisonReason::ImageMismatch,
                description: "Image identity, geometry or type differs".into(),
            });
        }
    }
    let mut used = vec![false; output_facts.records.len()];
    let mut output_index: BTreeMap<(String, Option<usize>, u8), VecDeque<usize>> = BTreeMap::new();
    let mut output_names = BTreeSet::new();
    for (index, record) in output_facts.records.iter().enumerate() {
        let family = kind_family(record.kind);
        output_index
            .entry((record.identifier.clone(), record.association, family))
            .or_default()
            .push_back(index);
        output_names.insert((record.identifier.clone(), family));
    }
    let source_keys: BTreeSet<_> = source_facts
        .records
        .iter()
        .map(|record| {
            (
                record.identifier.clone(),
                record.association,
                kind_family(record.kind),
            )
        })
        .collect();
    let mut allowed_additions: BTreeMap<(Option<usize>, String, String), usize> = BTreeMap::new();
    if let Some(expected) = provenance {
        if expected.output_format != output.format {
            bail!("Provenance destination format differs from output");
        }
        if output.format == ContainerFormat::Fits {
            for image in &output_facts.images {
                *allowed_additions
                    .entry((
                        Some(image.index),
                        "CREATOR".into(),
                        format!("AstroMuninn {}", expected.application_version),
                    ))
                    .or_default() += 1;
                for history in expected.history_lines() {
                    *allowed_additions
                        .entry((Some(image.index), "HISTORY".into(), history))
                        .or_default() += 1;
                }
            }
        } else {
            for (id, value) in expected.xisf_properties() {
                *allowed_additions.entry((None, id, value)).or_default() += 1;
            }
        }
    }
    for (record, impact) in source_facts.records.iter().zip(
        ledger
            .items
            .iter()
            .filter(|item| !matches!(item.source, SourceRef::Image { .. })),
    ) {
        validate_impact(record, impact)?;
        if matches!(impact.state, ImpactState::Lost | ImpactState::Unsupported)
            || record.role == SemanticRole::Structural
        {
            continue;
        }
        let key = (
            record.identifier.clone(),
            record.association,
            kind_family(record.kind),
        );
        let candidate = output_index.get_mut(&key).and_then(VecDeque::pop_front);
        if let Some(index) = candidate {
            let other = &output_facts.records[index];
            used[index] = true;
            if !equivalent_type(record, other) {
                differences.push(diff(record, ComparisonReason::TypeMismatch));
            } else if !equivalent_text(record, other)
                || record.comment != other.comment
                || record.binary_length != other.binary_length
            {
                differences.push(diff(record, ComparisonReason::ValueMismatch));
            } else if let Some(length) = record.binary_length {
                if let Some((source_path, output_path, limit)) = paths {
                    let (
                        SourceRef::XisfElement {
                            element: source_element,
                        },
                        SourceRef::XisfElement {
                            element: output_element,
                        },
                    ) = (record.source, other.source)
                    else {
                        differences.push(diff(record, ComparisonReason::UnsupportedComparison));
                        continue;
                    };
                    let mut left = vec![0u8; limit.min(length as usize)];
                    let mut right = vec![0u8; left.len()];
                    let mut position = 0u64;
                    while position < length {
                        let size = (length - position).min(limit as u64) as usize;
                        read_ancillary_chunk(
                            source_path,
                            source,
                            AncillaryReference::XisfElement(source_element),
                            position,
                            &mut left[..size],
                            limit,
                        )?;
                        read_ancillary_chunk(
                            output_path,
                            output,
                            AncillaryReference::XisfElement(output_element),
                            position,
                            &mut right[..size],
                            limit,
                        )?;
                        if left[..size] != right[..size] {
                            differences.push(diff(record, ComparisonReason::ValueMismatch));
                            break;
                        }
                        position += size as u64;
                    }
                } else {
                    differences.push(diff(record, ComparisonReason::UnsupportedComparison));
                }
            }
        } else if output_names.contains(&(record.identifier.clone(), kind_family(record.kind))) {
            differences.push(diff(record, ComparisonReason::AssociationMismatch));
        } else {
            differences.push(diff(record, ComparisonReason::MissingProperty));
        }
    }
    for (index, other) in output_facts.records.iter().enumerate() {
        if used[index] || other.role == SemanticRole::Structural {
            continue;
        }
        let allowance_key = (
            other.association,
            other.identifier.clone(),
            other.text.clone().unwrap_or_default(),
        );
        if let Some(remaining) = allowed_additions.get_mut(&allowance_key) {
            if *remaining > 0 {
                *remaining -= 1;
                continue;
            }
        }
        let duplicate = source_keys.contains(&(
            other.identifier.clone(),
            other.association,
            kind_family(other.kind),
        ));
        differences.push(MetadataDifference {
            source: other.source,
            identifier: other.identifier.clone(),
            reason: ComparisonReason::UnexpectedProperty,
            description: format!(
                "Unexpected {}metadata: {}",
                if duplicate { "duplicate " } else { "" },
                other.identifier
            ),
        });
    }
    if ledger.eligibility == Eligibility::Blocked {
        differences.push(MetadataDifference {
            source: SourceRef::Image { image: 0 },
            identifier: "eligibility".into(),
            reason: ComparisonReason::UnsupportedComparison,
            description: "Source ledger contains an unsupported mapping".into(),
        });
    }
    if let Some(expected) = provenance {
        let check = verify_provenance(output, expected);
        for problem in check.differences {
            differences.push(MetadataDifference {
                source: SourceRef::Image { image: 0 },
                identifier: "provenance".into(),
                reason: ComparisonReason::MissingProperty,
                description: problem,
            });
        }
    }
    Ok(ComparisonReport {
        passed: differences.is_empty(),
        differences,
    })
}

fn validate_impact(record: &SemanticRecord, impact: &ImpactItem) -> Result<()> {
    if record.source != impact.source
        || record.association != impact.association
        || record.identifier != impact.identifier
        || record.declared_type != impact.original_type
        || record.text != impact.original_value
        || record.comment != impact.original_comment
        || record.binary_length != impact.original_binary_length
    {
        bail!("Ledger source record differs from inventory");
    }
    Ok(())
}

fn diff(record: &SemanticRecord, reason: ComparisonReason) -> MetadataDifference {
    MetadataDifference {
        source: record.source,
        identifier: record.identifier.clone(),
        reason,
        description: format!("{}: {}", reason.as_str(), record.identifier),
    }
}
fn canonical_axes(format: ContainerFormat, axes: &[u64]) -> Vec<u64> {
    if format == ContainerFormat::Fits && axes.len() == 2 {
        vec![axes[0], axes[1], 1]
    } else {
        axes.to_vec()
    }
}
fn canonical_image_id(format: ContainerFormat, image: &SemanticImage) -> Option<&str> {
    if format == ContainerFormat::Fits
        && image.index == 0
        && image.id.as_deref() == Some("_PRIMARY")
    {
        None
    } else {
        image.id.as_deref()
    }
}
fn kind_family(kind: RecordKind) -> u8 {
    match kind {
        RecordKind::FitsCard | RecordKind::FitsKeyword => 0,
        RecordKind::FitsHdu => 1,
        RecordKind::Property => 2,
        RecordKind::Ancillary => 3,
        RecordKind::Core => 4,
    }
}
fn equivalent_type(left: &SemanticRecord, right: &SemanticRecord) -> bool {
    left.declared_type == right.declared_type
}
fn equivalent_text(left: &SemanticRecord, right: &SemanticRecord) -> bool {
    match (left.text.as_deref(), right.text.as_deref()) {
        (None, None) => true,
        (Some(a), Some(b))
            if matches!(left.kind, RecordKind::FitsCard | RecordKind::FitsKeyword)
                && matches!(right.kind, RecordKind::FitsCard | RecordKind::FitsKeyword) =>
        {
            let a = normalize_fits_value(a);
            let b = normalize_fits_value(b);
            a == b
                || decimal_key(&a)
                    .zip(decimal_key(&b))
                    .is_some_and(|(a, b)| a == b)
        }
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
fn normalize_fits_value(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(inner) = trimmed
        .strip_prefix('\'')
        .and_then(|text| text.strip_suffix('\''))
    {
        inner.replace("''", "'").trim_end().to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Exact decimal normalization without floating-point rounding. It deliberately
/// caps exponents and rejects NaN/Inf; ambiguous numeric spelling is unequal.
fn decimal_key(input: &str) -> Option<(bool, String, i32)> {
    let text = input.trim();
    let (negative, text) = if let Some(rest) = text.strip_prefix('-') {
        (true, rest)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let (mantissa, exponent) = match text.split_once(['e', 'E', 'd', 'D']) {
        Some((mantissa, power)) => (mantissa, power.parse::<i32>().ok()?),
        None => (text, 0),
    };
    if !(-1000..=1000).contains(&exponent) {
        return None;
    }
    let mut digits = String::new();
    let mut fractional = 0i32;
    let mut dot = false;
    for ch in mantissa.chars() {
        if ch == '.' && !dot {
            dot = true;
        } else if ch.is_ascii_digit() {
            digits.push(ch);
            if dot {
                fractional += 1;
            }
        } else {
            return None;
        }
    }
    if digits.is_empty() {
        return None;
    }
    let digits = digits.trim_start_matches('0').to_owned();
    if digits.is_empty() {
        return Some((false, "0".into(), 0));
    }
    let trailing = digits
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'0')
        .count();
    let exponent = exponent
        .checked_sub(fractional)?
        .checked_add(i32::try_from(trailing).ok()?)?;
    Some((
        negative,
        digits[..digits.len() - trailing].to_owned(),
        exponent,
    ))
}

/// Stable category counts for future versioned JSON reports.
pub fn loss_counts(ledger: &ImpactLedger) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for entry in &ledger.items {
        if let Some(category) = entry.category {
            *counts.entry(category.as_str()).or_insert(0) += 1;
        }
    }
    counts
}
