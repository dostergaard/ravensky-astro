//! Format-specific truthful provenance fields. P4 owns staging and final
//! verification; this module only serializes facts after caller-supplied
//! preliminary readback evidence and checks the independently inventoried bytes.

use super::{ComparisonReport, LossCategory};
use anyhow::{bail, Result};
use astro_io::typed::{compare::ImageMismatch, ContainerFormat, ContainerInventory};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct PreliminaryVerified {
    _private: (),
}
impl PreliminaryVerified {
    /// The caller must supply the real P1 pixel comparison and P2 metadata
    /// readback results for the same staged output. This token has no filesystem
    /// authority; P4 must still verify final bytes after provenance is written.
    pub fn from_readback(
        pixel_mismatch: Option<ImageMismatch>,
        metadata: &ComparisonReport,
    ) -> Result<Self> {
        if pixel_mismatch.is_some() || !metadata.passed {
            bail!("Preliminary image and metadata readback must pass before verified provenance");
        }
        Ok(Self { _private: () })
    }
}

#[derive(Debug, Clone)]
pub struct ProvenanceInput<'a> {
    pub application_version: &'a str,
    pub source_format: ContainerFormat,
    pub output_format: ContainerFormat,
    pub actual_method: &'a str,
    pub operation_id: Option<&'a str>,
    /// P3 supplies explicitly approved categories; P2 does not grant them.
    pub approved_losses: &'a [LossCategory],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceRecord {
    pub application: String,
    pub application_version: String,
    pub source_format: ContainerFormat,
    pub output_format: ContainerFormat,
    pub actual_method: String,
    pub operation_id: Option<String>,
    pub verified: bool,
    pub approved_losses: Vec<LossCategory>,
}

impl ProvenanceRecord {
    pub fn from_verified(input: ProvenanceInput<'_>, _: PreliminaryVerified) -> Result<Self> {
        if input.application_version.is_empty()
            || input.application_version.len() > 20
            || !safe_ascii(input.application_version)
            || input.actual_method.is_empty()
            || input.actual_method.len() > 32
            || !safe_ascii(input.actual_method)
            || input
                .operation_id
                .is_some_and(|id| id.is_empty() || id.len() > 32 || !safe_ascii(id))
        {
            bail!("Provenance version, method or operation ID is invalid");
        }
        let mut approved_losses = input.approved_losses.to_vec();
        approved_losses.sort_by_key(|category| category.as_str());
        approved_losses.dedup();
        Ok(Self {
            application: "AstroMuninn".into(),
            application_version: input.application_version.into(),
            source_format: input.source_format,
            output_format: input.output_format,
            actual_method: input.actual_method.into(),
            operation_id: input.operation_id.map(str::to_owned),
            verified: true,
            approved_losses,
        })
    }

    /// Append these cards to every output image HDU with P1's checked raw-card
    /// writer. Existing source CREATOR/HISTORY cards must be retained first.
    pub fn fits_cards(&self) -> Result<Vec<String>> {
        if self.output_format != ContainerFormat::Fits {
            bail!("Provenance destination is not FITS");
        }
        let creator = format!("AstroMuninn {}", self.application_version);
        let mut cards = vec![value_card("CREATOR", &format!("'{}'", creator))?];
        for history in self.history_lines() {
            cards.push(commentary_card("HISTORY", &history)?);
        }
        Ok(cards)
    }

    /// Root-level application namespace properties for the P1 XISF writer.
    /// `XISF:` is reserved and is deliberately not used.
    pub fn xisf_properties(&self) -> Vec<(String, String)> {
        let mut properties = vec![
            ("AstroMuninn:Application".into(), self.application.clone()),
            (
                "AstroMuninn:Version".into(),
                self.application_version.clone(),
            ),
            (
                "AstroMuninn:SourceFormat".into(),
                format_name(self.source_format).into(),
            ),
            (
                "AstroMuninn:OutputFormat".into(),
                format_name(self.output_format).into(),
            ),
            ("AstroMuninn:Method".into(), self.actual_method.clone()),
            ("AstroMuninn:Verification".into(), "passed".into()),
            (
                "AstroMuninn:ApprovedLossCategories".into(),
                self.approved_losses
                    .iter()
                    .map(|category| category.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ];
        if let Some(id) = &self.operation_id {
            properties.push(("AstroMuninn:OperationId".into(), id.clone()));
        }
        properties
    }

    pub(super) fn history_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "AstroMuninn source={} output={}",
                format_name(self.source_format),
                format_name(self.output_format)
            ),
            format!("AstroMuninn method={}", self.actual_method),
            "AstroMuninn verification=passed".into(),
        ];
        if let Some(id) = &self.operation_id {
            lines.push(format!("AstroMuninn operation={id}"));
        }
        for category in &self.approved_losses {
            lines.push(format!("AstroMuninn approved-loss={}", category.as_str()));
        }
        lines
    }
}

fn safe_ascii(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| (32..=126).contains(&byte) && byte != b'\'')
}
fn format_name(format: ContainerFormat) -> &'static str {
    match format {
        ContainerFormat::Fits => "FITS",
        ContainerFormat::Xisf => "XISF",
    }
}
fn value_card(name: &str, literal: &str) -> Result<String> {
    let mut card = format!("{name:<8}= {literal}");
    if card.len() > 80 {
        bail!("Provenance FITS value card too long");
    }
    card.extend(std::iter::repeat_n(' ', 80 - card.len()));
    Ok(card)
}
fn commentary_card(name: &str, text: &str) -> Result<String> {
    if text.len() > 72 || !safe_ascii(text) {
        bail!("Provenance FITS HISTORY card too long or invalid");
    }
    let mut card = format!("{name:<8}{text}");
    card.extend(std::iter::repeat_n(' ', 80 - card.len()));
    Ok(card)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceCheck {
    pub passed: bool,
    pub differences: Vec<String>,
}

/// Check the actual output inventory, including every image HDU for FITS.
/// P4 must additionally perform a final full container/pixel verification.
pub fn verify_provenance(
    output: &ContainerInventory,
    expected: &ProvenanceRecord,
) -> ProvenanceCheck {
    let mut differences = Vec::new();
    if output.format != expected.output_format {
        differences.push("output-format".into());
    } else if output.format == ContainerFormat::Fits {
        let creator = format!("AstroMuninn {}", expected.application_version);
        for image in &output.images {
            let Some(hdu) = output.fits_hdus.get(image.source_index) else {
                differences.push(format!("missing-hdu:{}", image.source_index));
                continue;
            };
            if !hdu
                .cards
                .iter()
                .any(|card| card.keyword == "CREATOR" && card.value.as_deref() == Some(&creator))
            {
                differences.push(format!("creator-hdu:{}", hdu.index));
            }
            for line in expected.history_lines() {
                if !hdu.cards.iter().any(|card| {
                    card.keyword == "HISTORY"
                        && card.raw_card.as_deref().is_some_and(|raw| {
                            raw.get(8..).is_some_and(|text| text.trim_end() == line)
                        })
                }) {
                    differences.push(format!("history-hdu:{}:{line}", hdu.index));
                }
            }
        }
    } else {
        for (id, value) in expected.xisf_properties() {
            if !output.xisf_elements.iter().any(|element| {
                element.name == "Property"
                    && element.parent_image_index.is_none()
                    && element.parent_index == Some(0)
                    && element.attributes.get("id") == Some(&id)
                    && element.attributes.get("type").map(String::as_str) == Some("String")
                    && element
                        .attributes
                        .get("value")
                        .map(String::as_str)
                        .or_else(|| (!element.text.is_empty()).then_some(element.text.as_str()))
                        == Some(value.as_str())
            }) {
                differences.push(format!("property:{id}"));
            }
        }
    }
    ProvenanceCheck {
        passed: differences.is_empty(),
        differences,
    }
}
