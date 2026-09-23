//! Private structural foundation for the monolithic XISF format.
//!
//! This module is the single implementation of the safe structural
//! interpretation of XISF shared by `super::load_xisf` and
//! `crate::validation::xisf`: the 16-byte file prefix, checked byte ranges,
//! strict XML syntax events, and image/storage descriptors.
//!
//! It stops at the parsing-versus-policy boundary. Every check here exists
//! only to make the result unambiguous and bounded: overflow, out-of-range
//! extents, invalid UTF-8, and malformed XML are *structural* failures and
//! fail every consumer identically. Conformance *policy* — which elements
//! are mandatory or unique, where they may appear, checksum and codec rules,
//! resource budgets — is deliberately absent from this module; the validator
//! layers that policy onto the same events and descriptors, and the loader
//! enforces its own narrower capability subset.
//!
//! The XML surface is a streaming event/descriptor layer, not a document
//! model: `visit_xml` hands owned events to a consumer one at a time, so no
//! consumer materializes an intermediate event vector or a general XISF AST,
//! and `quick-xml` types never escape this module. This stage intentionally
//! adds no public XISF parser API; the raw-record handoff to
//! `astro-metadata` is a later stage.

use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use std::{collections::BTreeMap, fmt, ops::Range};

/// The 8-byte signature of the XISF 1.00 monolithic prefix: `XISF` plus two
/// major and two minor version digits.
pub(crate) const SIGNATURE: &[u8; 8] = b"XISF0100";

/// Total prefix size: signature, a 4-byte little-endian XML-header length,
/// and 4 reserved bytes that must read zero. The historical 12-byte prefix
/// (XML beginning at offset 12) is a format defect, not an alternate legacy
/// form: files written with it are rejected here and must not receive a
/// compatibility path.
pub(crate) const PREFIX_LEN: u64 = 16;

/// The canonical XISF namespace. Unprefixed names are also treated as core
/// by `Namespace::is_core` to stay compatible with common producers and
/// existing fixtures.
const CORE_NAMESPACE: &[u8] = b"http://www.pixinsight.com/xisf";

/// Failure categories of structural interpretation.
///
/// `Incomplete` means bytes a declaration requires are absent (truncated
/// prefix, range past the source, unclosed document); `Invalid` means the
/// bytes that are present contradict the format (signature, malformed XML,
/// overflow); `Unsupported` means the input is well-formed but outside what
/// a structural reader interprets (DTDs, unknown sample formats).
/// Keeping the categories distinct is part of the diagnostic contract:
/// consumers must tell malformed input apart from merely unsupported input,
/// and `crate::validation::xisf` maps these one-to-one onto
/// `ValidationErrorKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    Incomplete,
    Invalid,
    Unsupported,
}

/// A structural failure: a stable `kind` plus a human-readable message.
#[derive(Debug)]
pub(crate) struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub(crate) fn kind(&self) -> ErrorKind {
        self.kind
    }

    fn incomplete(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Incomplete,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Invalid,
            message: message.into(),
        }
    }

    fn unsupported(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Unsupported,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// The checked layout of a monolithic XISF file: the XML header's range in
/// the observed source.
///
/// This is the single place where the 16-byte prefix is interpreted, so the
/// loader and the validator share one prefix dialect. Parsing is
/// stage-by-stage and each missing stage is reported by name — signature,
/// XML-header length, reserved area — instead of a vague "short file".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonolithicEnvelope {
    xml: Range<u64>,
}

impl MonolithicEnvelope {
    /// Parse the monolithic prefix against the observed source length.
    ///
    /// `source_len` is the file's actual size, so the returned XML range is
    /// always within the file: a declared length that runs past it is
    /// `Incomplete`, and a short prefix is `Incomplete` with the specific
    /// stage named. Only a prefix that is present but contradictory is
    /// `Invalid`.
    pub(crate) fn parse(prefix: &[u8], source_len: u64) -> Result<Self, Error> {
        if prefix.len() < SIGNATURE.len() {
            return Err(Error::incomplete("truncated XISF signature"));
        }
        if &prefix[..SIGNATURE.len()] != SIGNATURE {
            return Err(Error::invalid("invalid XISF signature"));
        }
        if prefix.len() < 12 {
            return Err(Error::incomplete("truncated XISF XML header length field"));
        }
        if prefix.len() < PREFIX_LEN as usize {
            return Err(Error::incomplete("truncated XISF reserved prefix area"));
        }
        // The reserved area must read zero. This check is also what makes
        // the defective 12-byte prefix fail loudly: a file that packed XML
        // directly after the length field has XML text, not zeros, here.
        if prefix[12..16] != [0; 4] {
            return Err(Error::invalid("XISF reserved prefix bytes must be zero"));
        }
        let xml_len = u32::from_le_bytes(
            prefix[8..12]
                .try_into()
                .map_err(|_| Error::invalid("invalid XISF XML header length"))?,
        ) as u64;
        let xml = checked_range(PREFIX_LEN, xml_len, source_len, "XISF XML header")?;
        Ok(Self { xml })
    }

    pub(crate) fn xml_range(&self) -> Range<u64> {
        self.xml.clone()
    }

    pub(crate) fn xml_len(&self) -> u64 {
        self.xml.end - self.xml.start
    }

    pub(crate) fn xml_end(&self) -> u64 {
        self.xml.end
    }
}

/// Build a checked `start..start + length` range bounded to the observed
/// source length.
///
/// An arithmetic overflow is `Invalid` (the declared range is
/// self-contradictory) while a range running past the source is `Incomplete`
/// (the file is missing bytes it declares). These are parsing checks even
/// though they reject input: they exist so that no consumer repeats unchecked
/// offset arithmetic or reads out-of-range bytes.
pub(crate) fn checked_range(
    start: u64,
    length: u64,
    source_len: u64,
    description: &str,
) -> Result<Range<u64>, Error> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| Error::invalid(format!("{description} range overflows")))?;
    if end > source_len {
        return Err(Error::incomplete(format!(
            "truncated {description}: range {start}..{end} exceeds source length {source_len}"
        )));
    }
    Ok(start..end)
}

/// The namespace of an XML name, reduced to the only facts consumers need.
///
/// `Unbound` (unprefixed) and `Core` (bound to the canonical namespace) are
/// both accepted as core: unprefixed names inherit only a *default* namespace
/// in XML, so a bare `Image` under a qualified root is still core and remains
/// a supported producer form. `Foreign` carries the resolved URI as a fact;
/// the structural layer neither accepts nor rejects foreign namespaces —
/// extension acceptance is consumer policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Namespace {
    Unbound,
    Core,
    Foreign(String),
}

impl Namespace {
    /// Whether this namespace is part of the core vocabulary (see the type
    /// documentation for why unbound names count).
    pub(crate) fn is_core(&self) -> bool {
        matches!(self, Self::Unbound | Self::Core)
    }
}

/// A start or empty element: local name, resolved namespace, attribute map,
/// depth, and self-closing flag.
///
/// Duplicate attributes are a structural error at parse time (XML 1.0
/// forbids them); the map is sorted so consumers iterate in deterministic
/// order, and the strings are owned so they outlive the reader.
#[derive(Debug)]
pub(crate) struct Element {
    pub(crate) name: String,
    pub(crate) namespace: Namespace,
    pub(crate) attrs: BTreeMap<String, String>,
    pub(crate) depth: usize,
    pub(crate) empty: bool,
}

impl Element {
    pub(crate) fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}

/// The owned, format-specific events emitted by `visit_xml`, delivered one
/// at a time with no document tree in between.
#[derive(Debug)]
pub(crate) enum XmlEvent {
    Element(Element),
    Text { value: String, depth: usize },
    End { name: String, depth: usize },
}

/// A failure while consuming a `visit_xml` stream.
///
/// `Structural` is produced by the reader (UTF-8, well-formedness, namespaces,
/// roots) and is identical for every consumer; `Consumer` carries the caller's
/// own policy error. The split lets each consumer map shared structural
/// failures onto its own error type without losing the structural taxonomy.
#[derive(Debug)]
pub(crate) enum VisitError<E> {
    Structural(Error),
    Consumer(E),
}

/// Reduce a `quick-xml` resolved namespace to `Namespace`.
///
/// `Unbound` is the common producer form and stays valid. `Unknown` — a
/// prefixed name with no corresponding `xmlns:prefix` declaration — is not
/// well-formed XML and is invalid; this is in particular what makes a
/// mistyped prefix fail here instead of being silently reinterpreted as core.
fn namespace(result: ResolveResult<'_>) -> Result<Namespace, Error> {
    match result {
        ResolveResult::Unbound => Ok(Namespace::Unbound),
        ResolveResult::Bound(value) if value.as_ref() == CORE_NAMESPACE => Ok(Namespace::Core),
        ResolveResult::Bound(value) => std::str::from_utf8(value.as_ref())
            .map(|value| Namespace::Foreign(value.to_string()))
            .map_err(|_| Error::invalid("invalid XML namespace UTF-8")),
        ResolveResult::Unknown(_) => Err(Error::invalid("unbound XML namespace")),
    }
}

/// Visit the XISF XML header as a strict, streaming, namespace-aware event
/// stream.
///
/// The whole header must be valid UTF-8 before any event is emitted — the
/// format forbids the lossy fallbacks older readers used — and the document
/// must contain exactly one core `xisf` root. Events are handed to `consumer`
/// one at a time with no intermediate vector, so a consumer can enforce its
/// own budgets and cancellation between events, and a huge but well-formed
/// document is rejected at the first violating event rather than after being
/// materialized as a tree.
///
/// Structural rules enforced here, independently of any consumer:
/// well-formedness (stray or mismatched closing tags), duplicate attributes,
/// significant text or CDATA outside the root (whitespace-only text outside
/// the root is insignificant and is skipped), CDATA folded into `Text`
/// events, and DTD/entity declarations reported as `Unsupported` rather than
/// `Invalid` — a `DOCTYPE` is well-formed XML, just a feature this reader
/// does not interpret, and the diagnostic must say so.
///
/// What this deliberately does *not* check — element presence, placement,
/// `version`, `uid` uniqueness, checksums, codecs, and resource limits — is
/// conformance policy that belongs to the consumer.
pub(crate) fn visit_xml<E>(
    xml: &[u8],
    mut consumer: impl FnMut(XmlEvent) -> Result<(), E>,
) -> Result<(), VisitError<E>> {
    // Strict UTF-8 over the whole header, checked once before any event. A
    // per-event check would let invalid bytes earlier in the document
    // generate events before the failure is reported.
    std::str::from_utf8(xml).map_err(|error| {
        VisitError::Structural(Error::invalid(format!(
            "invalid XISF XML UTF-8 at byte {}",
            error.valid_up_to()
        )))
    })?;

    let mut reader = NsReader::from_reader(xml);
    let mut stack: Vec<(String, Namespace)> = Vec::new();
    let mut roots = 0usize;
    loop {
        let (resolved, event) = reader.read_resolved_event().map_err(|error| {
            VisitError::Structural(Error::invalid(format!("invalid XISF XML: {error}")))
        })?;
        match event {
            Event::Start(ref raw) | Event::Empty(ref raw) => {
                let namespace = namespace(resolved).map_err(VisitError::Structural)?;
                let name = std::str::from_utf8(raw.local_name().as_ref())
                    .map_err(|_| VisitError::Structural(Error::invalid("invalid element name")))?
                    .to_string();
                let depth = stack.len();
                if depth == 0 {
                    roots += 1;
                    // Exactly one core `xisf` root: a second root, a root
                    // with another name, or a root bound to a foreign
                    // namespace is an invalid document, not an extension
                    // point (root extension acceptance is a later stage).
                    if roots != 1 || name != "xisf" || !namespace.is_core() {
                        return Err(VisitError::Structural(Error::invalid(
                            "expected one core xisf XML root",
                        )));
                    }
                }
                let mut attrs = BTreeMap::new();
                for attribute in raw.attributes() {
                    let attribute = attribute.map_err(|error| {
                        VisitError::Structural(Error::invalid(format!(
                            "invalid XML attribute: {error}"
                        )))
                    })?;
                    let key = std::str::from_utf8(attribute.key.as_ref())
                        .map_err(|_| {
                            VisitError::Structural(Error::invalid("invalid attribute name"))
                        })?
                        .to_string();
                    let value = attribute
                        .unescape_value()
                        .map_err(|error| {
                            VisitError::Structural(Error::invalid(format!(
                                "invalid XML attribute value: {error}"
                            )))
                        })?
                        .into_owned();
                    if attrs.insert(key, value).is_some() {
                        return Err(VisitError::Structural(Error::invalid(
                            "duplicate XML attribute",
                        )));
                    }
                }
                let empty = matches!(event, Event::Empty(_));
                consumer(XmlEvent::Element(Element {
                    name: name.clone(),
                    namespace: namespace.clone(),
                    attrs,
                    depth,
                    empty,
                }))
                .map_err(VisitError::Consumer)?;
                if !empty {
                    stack.push((name, namespace));
                }
            }
            Event::End(raw) => {
                let namespace = namespace(resolved).map_err(VisitError::Structural)?;
                let name = std::str::from_utf8(raw.local_name().as_ref())
                    .map_err(|_| VisitError::Structural(Error::invalid("invalid element name")))?
                    .to_string();
                let Some((expected, expected_namespace)) = stack.pop() else {
                    return Err(VisitError::Structural(Error::invalid(
                        "unexpected XML closing element",
                    )));
                };
                if name != expected || namespace != expected_namespace {
                    return Err(VisitError::Structural(Error::invalid(format!(
                        "mismatched XML closing element: expected {expected}, got {name}"
                    ))));
                }
                consumer(XmlEvent::End {
                    name,
                    depth: stack.len(),
                })
                .map_err(VisitError::Consumer)?;
            }
            Event::Text(raw) => {
                let value = raw
                    .unescape()
                    .map_err(|error| {
                        VisitError::Structural(Error::invalid(format!("invalid XML text: {error}")))
                    })?
                    .into_owned();
                if stack.is_empty() {
                    // Text outside the root is insignificant only while it is
                    // whitespace; significant text there is a structural
                    // error.
                    if !value.trim().is_empty() {
                        return Err(VisitError::Structural(Error::invalid(
                            "text outside XML root",
                        )));
                    }
                } else {
                    consumer(XmlEvent::Text {
                        value,
                        depth: stack.len(),
                    })
                    .map_err(VisitError::Consumer)?;
                }
            }
            Event::CData(raw) => {
                let value = std::str::from_utf8(raw.as_ref())
                    .map_err(|_| VisitError::Structural(Error::invalid("invalid XML UTF-8")))?
                    .to_string();
                if stack.is_empty() {
                    return Err(VisitError::Structural(Error::invalid(
                        "CDATA outside XML root",
                    )));
                }
                consumer(XmlEvent::Text {
                    value,
                    depth: stack.len(),
                })
                .map_err(VisitError::Consumer)?;
            }
            Event::DocType(_) => {
                return Err(VisitError::Structural(Error::unsupported(
                    "XML DTD/entity declarations are not supported",
                )));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    // A complete document has exactly one root and an empty element stack
    // at EOF; either failure means the header is truncated or unbalanced.
    if roots != 1 || !stack.is_empty() {
        return Err(VisitError::Structural(Error::invalid(
            "incomplete XISF XML document",
        )));
    }
    Ok(())
}

/// An image's dimension vector in declared order (conventionally
/// `width:height:channels`).
///
/// The structural layer only requires at least two dimensions — at least one
/// spatial and one channel; consumers may impose stricter shapes (the loader
/// requires exactly three).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Geometry {
    dimensions: Vec<u64>,
}

impl Geometry {
    pub(crate) fn dimensions(&self) -> &[u64] {
        &self.dimensions
    }
}

/// Where a block's bytes come from, as a location *descriptor* rather than
/// materialized bytes: `Attachment` is a byte range in the same file,
/// `Inline` is encoded text in the element, `Embedded` is a child `Data`
/// element, and `Other` records an unrecognized location spelling as a raw
/// fact so consumers can report it unsupported instead of guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockLocation {
    Attachment { offset: u64, size: u64 },
    Inline { encoding: String },
    Embedded,
    Other(String),
}

/// A parsed `Image` or `Thumbnail` descriptor: geometry, sample format,
/// optional byte order, storage location, and optional compression.
///
/// Parsing is syntax-complete but policy-light: it requires only what any
/// image consumer needs, and `byte_order` is preserved verbatim — Stage 1
/// consumers deliberately do not decode with it, and byte-order-correct
/// decoding is a later stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageDescriptor {
    pub(crate) geometry: Geometry,
    pub(crate) sample_format: String,
    pub(crate) byte_order: Option<String>,
    pub(crate) location: BlockLocation,
    pub(crate) compression: Option<String>,
}

impl ImageDescriptor {
    pub(crate) fn parse(element: &Element) -> Result<Self, Error> {
        if !matches!(element.name.as_str(), "Image" | "Thumbnail") {
            return Err(Error::invalid(format!(
                "{} is not an image element",
                element.name
            )));
        }
        let geometry = element
            .attr("geometry")
            .ok_or_else(|| Error::invalid(format!("{} missing geometry", element.name)))?;
        let dimensions = geometry
            .split(':')
            .map(|part| {
                let value = part
                    .parse::<u64>()
                    .map_err(|_| Error::invalid(format!("invalid image dimension '{part}'")))?;
                if value == 0 {
                    return Err(Error::invalid("zero image dimension"));
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if dimensions.len() < 2 {
            return Err(Error::invalid(
                "image geometry needs spatial dimensions and channels",
            ));
        }
        let sample_format = element
            .attr("sampleFormat")
            .ok_or_else(|| Error::invalid(format!("{} missing sampleFormat", element.name)))?
            .to_string();
        let location = BlockLocation::parse(
            element
                .attr("location")
                .ok_or_else(|| Error::invalid(format!("{} missing location", element.name)))?,
        )?;
        Ok(Self {
            geometry: Geometry { dimensions },
            sample_format,
            byte_order: element.attr("byteOrder").map(str::to_string),
            location,
            compression: element.attr("compression").map(str::to_string),
        })
    }

    /// The raw byte count the geometry and sample format require, with
    /// checked multiplication, so a hostile dimension such as 2^64 − 1 is an
    /// `Invalid` error rather than a wrapped allocation size.
    pub(crate) fn expected_bytes(&self) -> Result<u64, Error> {
        let mut samples = 1u64;
        for dimension in self.geometry.dimensions() {
            samples = samples
                .checked_mul(*dimension)
                .ok_or_else(|| Error::invalid("image sample count overflows"))?;
        }
        samples
            .checked_mul(sample_size(&self.sample_format)?)
            .ok_or_else(|| Error::invalid("image byte count overflows"))
    }
}

impl BlockLocation {
    pub(crate) fn parse(value: &str) -> Result<Self, Error> {
        if let Some(rest) = value.strip_prefix("attachment:") {
            let (offset, size) = rest
                .split_once(':')
                .ok_or_else(|| Error::invalid("invalid attachment location"))?;
            let offset = offset
                .parse()
                .map_err(|_| Error::invalid(format!("invalid attachment offset '{offset}'")))?;
            let size = size
                .parse()
                .map_err(|_| Error::invalid(format!("invalid attachment size '{size}'")))?;
            Ok(Self::Attachment { offset, size })
        } else if let Some(encoding) = value.strip_prefix("inline:") {
            Ok(Self::Inline {
                encoding: encoding.to_string(),
            })
        } else if value == "embedded" {
            Ok(Self::Embedded)
        } else {
            Ok(Self::Other(value.to_string()))
        }
    }
}

/// Bytes per sample for the standard sample formats.
///
/// Unknown spellings are `Unsupported` rather than `Invalid`: the value is a
/// well-formed descriptor, and whether a given consumer can decode it is a
/// capability decision that this shared layer only surfaces.
fn sample_size(value: &str) -> Result<u64, Error> {
    match value {
        "UInt8" => Ok(1),
        "UInt16" => Ok(2),
        "UInt32" | "Float32" => Ok(4),
        "UInt64" | "Float64" | "Complex32" => Ok(8),
        "Complex64" => Ok(16),
        _ => Err(Error::unsupported(format!("sample format {value}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the *correct* 16-byte prefix for a header of the given length.
    /// There is deliberately no 12-byte variant: that form is a defect and
    /// must not become a fixture convention.
    fn prefix(xml_len: u32) -> [u8; 16] {
        let mut prefix = [0; 16];
        prefix[..8].copy_from_slice(SIGNATURE);
        prefix[8..12].copy_from_slice(&xml_len.to_le_bytes());
        prefix
    }

    #[test]
    fn monolithic_envelope_requires_the_complete_sixteen_byte_prefix() {
        let valid = prefix(4);
        assert_eq!(
            MonolithicEnvelope::parse(&valid, 20).unwrap().xml_range(),
            16..20
        );
        for (length, message) in [(7, "signature"), (11, "length"), (15, "reserved")] {
            let error = MonolithicEnvelope::parse(&valid[..length], 20).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Incomplete);
            assert!(error.to_string().contains(message));
        }
        let mut invalid_signature = valid;
        invalid_signature[0] = b'!';
        assert_eq!(
            MonolithicEnvelope::parse(&invalid_signature, 20)
                .unwrap_err()
                .kind(),
            ErrorKind::Invalid
        );
        let mut reserved = valid;
        reserved[12] = 1;
        assert!(MonolithicEnvelope::parse(&reserved, 20)
            .unwrap_err()
            .to_string()
            .contains("reserved"));
    }

    #[test]
    fn checked_ranges_reject_overflow_and_truncation() {
        assert_eq!(checked_range(16, 4, 20, "header").unwrap(), 16..20);
        assert_eq!(
            checked_range(u64::MAX, 1, u64::MAX, "header")
                .unwrap_err()
                .kind(),
            ErrorKind::Invalid
        );
        assert_eq!(
            checked_range(16, 5, 20, "header").unwrap_err().kind(),
            ErrorKind::Incomplete
        );
    }

    #[test]
    fn image_descriptor_checks_sample_byte_count_overflow() {
        let mut descriptor = None;
        visit_xml(
            br#"<xisf><Image geometry="18446744073709551615:2:1" sampleFormat="UInt16" location="attachment:128:8"/></xisf>"#,
            |event| {
                if let XmlEvent::Element(element) = event {
                    if element.name == "Image" {
                        descriptor = Some(ImageDescriptor::parse(&element).unwrap());
                    }
                }
                Ok::<_, ()>(())
            },
        )
        .unwrap();

        assert_eq!(
            descriptor.unwrap().expected_bytes().unwrap_err().kind(),
            ErrorKind::Invalid
        );
    }

    #[test]
    fn strict_xml_reports_utf8_roots_malformed_input_and_namespaces() {
        let error = visit_xml(&[0xff], |_| Ok::<_, ()>(())).unwrap_err();
        assert!(matches!(
            error,
            VisitError::Structural(Error {
                kind: ErrorKind::Invalid,
                ..
            })
        ));
        assert!(visit_xml(b"<not-xisf/>", |_| Ok::<_, ()>(())).is_err());
        assert!(visit_xml(b"<xisf><Image></xisf>", |_| Ok::<_, ()>(())).is_err());

        let mut facts = Vec::new();
        visit_xml(
            br#"<x:xisf xmlns:x="http://www.pixinsight.com/xisf"><x:Image geometry="1:1:1" sampleFormat="UInt8" location="attachment:128:1"><x:FITSKeyword name="A" value="B"/><x:Property id="P" type="String">v</x:Property></x:Image></x:xisf>"#,
            |event| {
                if let XmlEvent::Element(element) = event {
                    facts.push((element.name, element.namespace, element.depth));
                }
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        assert_eq!(facts[0], ("xisf".to_string(), Namespace::Core, 0));
        assert_eq!(facts[1], ("Image".to_string(), Namespace::Core, 1));
        assert_eq!(facts[2].0, "FITSKeyword");
        assert_eq!(facts[3].0, "Property");
    }
}
