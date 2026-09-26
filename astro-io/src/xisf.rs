//! XISF file access
//!
//! This module provides pixel loading and raw metadata-record access for XISF
//! files.
//! XISF (Extensible Image Serialization Format) is an XML-based format used by PixInsight.
//!
//! The current loader intentionally supports a narrow, explicit subset:
//! uncompressed, `zstd`, or `zstd+sh` single-channel, attachment-backed
//! `UInt16` images.
//! Malformed or unsupported files return an error instead of producing
//! placeholder image data.
//!
//! The monolithic prefix, XML syntax, and image/storage descriptors are
//! interpreted by the shared `structural` module — the same source the full
//! validator uses — so the loader has no second prefix dialect or
//! string-scanning parser of its own. This module only layers the loader's
//! capability policy on top: anything it cannot load exactly is rejected with
//! an explicit error.
//!
//! Byte order is resolved by the shared descriptor and applied explicitly by
//! the pixel decoder. The shared codec layer produces an exact uncompressed
//! byte block before sample conversion.

pub(crate) mod codec;
pub(crate) mod structural;

use anyhow::{anyhow, bail, Context, Result};
use log::debug;
use std::convert::Infallible;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use codec::{decode_zstd_frame, unshuffle, zstd_frame_requirements, BoundedBufRead, Compression};
use structural::{
    checked_range, visit_xml, BlockLocation, ByteOrder, ImageDescriptor, MonolithicEnvelope,
    VisitError, XmlEvent, PREFIX_LEN,
};

/// Conservative native-history ceiling for the compatibility loader. The
/// validator has caller-configurable shared admission; this legacy signature
/// does not, so it rejects frames requiring more than 128 MiB of history.
/// This matches Zstandard's default streaming-decoder window limit; it is not
/// an XISF format limit.
const MAX_LOADER_ZSTD_HISTORY: u64 = 128 << 20;

/// Read the complete monolithic XISF XML header as strict UTF-8.
///
/// This validates and reads the envelope through the shared structural path,
/// then returns the original XML text without projecting it into XISF metadata
/// records. Consumers that need to inspect writer-specific XML can therefore
/// retain unknown elements, attributes, comments, and ordering.
///
/// # Errors
///
/// Returns an error for I/O failures, an invalid or incomplete monolithic
/// envelope, a declared XML range that cannot be represented locally, or a
/// header that is not valid UTF-8.
pub fn read_xisf_xml_header<R: Read + Seek>(reader: &mut R) -> Result<String> {
    let header = read_monolithic_header(reader)?;
    match visit_xml(&header.xml, |_| Ok::<(), Infallible>(())) {
        Ok(()) => {}
        Err(VisitError::Structural(error)) => return Err(anyhow::Error::new(error)),
        Err(VisitError::Consumer(never)) => match never {},
    }
    String::from_utf8(header.xml).context("XISF XML header is not valid UTF-8")
}

/// Raw XISF metadata facts collected from the shared structural event stream.
///
/// These records deliberately preserve XML-decoded lexical strings rather
/// than applying astronomy-domain normalization. That semantic work belongs
/// in `astro-metadata`; `astro-io` owns only the file-format syntax. The API is
/// intentionally smaller than the XISF data model and exposes no parser or
/// validation implementation types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct XisfMetadataRecords {
    version: Option<String>,
    block_alignment: Option<String>,
    fits_keywords: Vec<XisfFitsKeyword>,
    properties: Vec<XisfProperty>,
    images: Vec<XisfImageMetadata>,
}

impl XisfMetadataRecords {
    /// Raw `xisf/@version` value, if present.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Raw historical `xisf/@blockAlignment` value, if present.
    pub fn block_alignment(&self) -> Option<&str> {
        self.block_alignment.as_deref()
    }

    /// Ordered FITS-style keyword records, including duplicate names.
    pub fn fits_keywords(&self) -> &[XisfFitsKeyword] {
        &self.fits_keywords
    }

    /// Ordered raw Property records.
    pub fn properties(&self) -> &[XisfProperty] {
        &self.properties
    }

    /// Ordered image records containing only attributes used by current
    /// metadata interpretation.
    pub fn images(&self) -> &[XisfImageMetadata] {
        &self.images
    }
}

/// One raw XISF `FITSKeyword` record.
///
/// Order and duplicates are retained because downstream FITS-compatible
/// precedence is semantic policy, not XML syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct XisfFitsKeyword {
    name: String,
    value: String,
    comment: Option<String>,
}

impl XisfFitsKeyword {
    /// Raw keyword name after XML entity decoding.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Raw keyword value after XML entity decoding, without FITS normalization.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Optional raw comment after XML entity decoding.
    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }
}

/// One raw XISF `Property` record needed by current metadata semantics.
///
/// `value` preserves either the scalar `value` attribute or direct element
/// text after XML entity decoding. It is not trimmed, typed, or otherwise
/// normalized here.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct XisfProperty {
    id: String,
    declared_type: String,
    value: Option<String>,
}

impl XisfProperty {
    /// Raw property identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Raw declared XISF property type.
    pub fn declared_type(&self) -> &str {
        &self.declared_type
    }

    /// Optional unnormalized scalar attribute or direct text value.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

/// Selected raw attributes from one XISF `Image` element.
///
/// Every field is optional because metadata inspection is independent of the
/// pixel loader's narrower capability requirements. No storage or codec model
/// is exposed through this cross-crate boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct XisfImageMetadata {
    id: Option<String>,
    geometry: Option<String>,
    sample_format: Option<String>,
    color_space: Option<String>,
    bits_per_sample: Option<String>,
    compression: Option<String>,
    compression_parameters: Option<String>,
    checksum_type: Option<String>,
    checksum: Option<String>,
    x_resolution: Option<String>,
    y_resolution: Option<String>,
    resolution_unit: Option<String>,
    display_function: Option<String>,
    display_parameters: Option<String>,
}

impl XisfImageMetadata {
    /// Optional raw image identifier.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Optional raw geometry descriptor.
    pub fn geometry(&self) -> Option<&str> {
        self.geometry.as_deref()
    }

    /// Optional raw sample-format name.
    pub fn sample_format(&self) -> Option<&str> {
        self.sample_format.as_deref()
    }

    /// Optional raw color-space name.
    pub fn color_space(&self) -> Option<&str> {
        self.color_space.as_deref()
    }

    /// Optional raw bits-per-sample value.
    pub fn bits_per_sample(&self) -> Option<&str> {
        self.bits_per_sample.as_deref()
    }

    /// Optional raw compression descriptor.
    pub fn compression(&self) -> Option<&str> {
        self.compression.as_deref()
    }

    /// Optional raw compression-parameter string.
    pub fn compression_parameters(&self) -> Option<&str> {
        self.compression_parameters.as_deref()
    }

    /// Optional raw checksum-algorithm name.
    pub fn checksum_type(&self) -> Option<&str> {
        self.checksum_type.as_deref()
    }

    /// Optional raw checksum value.
    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }

    /// Optional raw horizontal-resolution value.
    pub fn x_resolution(&self) -> Option<&str> {
        self.x_resolution.as_deref()
    }

    /// Optional raw vertical-resolution value.
    pub fn y_resolution(&self) -> Option<&str> {
        self.y_resolution.as_deref()
    }

    /// Optional raw resolution-unit name.
    pub fn resolution_unit(&self) -> Option<&str> {
        self.resolution_unit.as_deref()
    }

    /// Optional raw display-function name.
    pub fn display_function(&self) -> Option<&str> {
        self.display_function.as_deref()
    }

    /// Optional raw display-parameter string.
    pub fn display_parameters(&self) -> Option<&str> {
        self.display_parameters.as_deref()
    }
}

struct MonolithicHeader {
    xml: Vec<u8>,
    source_len: u64,
    xml_end: u64,
}

struct PendingProperty {
    record: XisfProperty,
    depth: usize,
    text: String,
    has_child: bool,
}

/// A resolved image data block: pixel dimensions plus the payload's extent in
/// the source file.
///
/// `data_offset`/`data_size` describe the attachment extent, and a block is
/// only produced after uncompressed storage matches the geometry exactly or a
/// compressed descriptor accounts for `data_size` and declares the exact
/// geometry-derived output size.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageDataBlock {
    width: usize,
    height: usize,
    data_offset: u64,
    data_size: usize,
    decoding: SampleDecoding,
    compression: Option<Compression>,
}

/// Bounded loader input whose logical consumption excludes read-ahead. It
/// never reads beyond one declared compression subblock.
struct ReaderInput<'a, R> {
    reader: &'a mut R,
    length: u64,
    consumed: u64,
    buffer: Vec<u8>,
    start: usize,
    end: usize,
}

impl<'a, R: Read> ReaderInput<'a, R> {
    fn new(reader: &'a mut R, length: u64) -> Result<Self> {
        let capacity = usize::try_from(length.min(65536))
            .context("XISF compressed input buffer exceeds address space")?;
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(capacity)
            .context("Failed to allocate XISF compressed input buffer")?;
        buffer.resize(capacity, 0);
        Ok(Self {
            reader,
            length,
            consumed: 0,
            buffer,
            start: 0,
            end: 0,
        })
    }
}

impl<R: Read> BufRead for ReaderInput<'_, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.start == self.end && self.consumed < self.length {
            let count = (self.length - self.consumed).min(self.buffer.len() as u64) as usize;
            self.reader.read_exact(&mut self.buffer[..count])?;
            self.start = 0;
            self.end = count;
        }
        Ok(&self.buffer[self.start..self.end])
    }

    fn consume(&mut self, amount: usize) {
        let amount = amount.min(self.end - self.start);
        self.start += amount;
        self.consumed += amount as u64;
    }
}

impl<R: Read> Read for ReaderInput<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let bytes = self.fill_buf()?;
        let count = bytes.len().min(output.len());
        output[..count].copy_from_slice(&bytes[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl<R: Read> BoundedBufRead for ReaderInput<'_, R> {
    fn logical_len(&self) -> u64 {
        self.length
    }

    fn logical_consumed(&self) -> u64 {
        self.consumed
    }
}

/// The primitive conversion selected for a structurally parsed sample format.
/// Keeping byte order in this value makes future sample-format additions pass
/// through the same explicit dispatch instead of falling back to native order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleDecoder {
    UInt16(ByteOrder),
}

/// Exact logical and byte lengths plus the conversion required to decode them.
/// These values are derived once from `ImageDescriptor`; the decoder never
/// reparses XML or reconstructs geometry from width and height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SampleDecoding {
    decoder: SampleDecoder,
    sample_count: usize,
    expected_bytes: usize,
}

impl SampleDecoding {
    fn from_descriptor(descriptor: &ImageDescriptor) -> Result<Self> {
        let decoder = match descriptor.sample_format.as_str() {
            "UInt16" => SampleDecoder::UInt16(descriptor.byte_order),
            sample_format => bail!(
                "Unsupported XISF sampleFormat '{sample_format}'; only UInt16 images are currently supported"
            ),
        };
        let layout = descriptor.sample_layout().map_err(anyhow::Error::new)?;
        let sample_count = usize::try_from(layout.sample_count())
            .context("XISF image sample count exceeds this platform's address space")?;
        let expected_bytes = usize::try_from(layout.byte_count())
            .context("XISF image byte count exceeds this platform's address space")?;
        Ok(Self {
            decoder,
            sample_count,
            expected_bytes,
        })
    }

    fn decode(self, data: &[u8]) -> Result<Vec<f32>> {
        if data.len() < self.expected_bytes {
            bail!(
                "XISF image payload is truncated: expected exactly {} bytes, got {}",
                self.expected_bytes,
                data.len()
            );
        }
        if data.len() > self.expected_bytes {
            bail!(
                "XISF image payload has trailing bytes: expected exactly {} bytes, got {}",
                self.expected_bytes,
                data.len()
            );
        }

        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.sample_count)
            .context("Failed to allocate XISF pixel output")?;
        match self.decoder {
            SampleDecoder::UInt16(byte_order) => {
                // Exact descriptor-derived length guarantees every chunk is a
                // complete sample. Explicit conversion makes the result
                // independent of the host architecture.
                let (samples, remainder) = data.as_chunks::<2>();
                debug_assert!(remainder.is_empty());
                for &bytes in samples {
                    let value = match byte_order {
                        ByteOrder::Little => u16::from_le_bytes(bytes),
                        ByteOrder::Big => u16::from_be_bytes(bytes),
                    };
                    pixels.push(f32::from(value) / 65535.0);
                }
            }
        }
        debug_assert_eq!(pixels.len(), self.sample_count);
        Ok(pixels)
    }
}

/// Read an XISF file and return its pixel data, width, and height.
///
/// The current implementation supports little- and big-endian uncompressed,
/// `zstd`, and `zstd+sh` single-channel, attachment-backed `UInt16` images.
///
/// Pixels are returned as `f32` values normalized to [0, 1]
/// (raw 16-bit sample / 65535).
pub fn load_xisf(path: &Path) -> Result<(Vec<f32>, usize, usize)> {
    debug!("Loading XISF file: {}", path.display());
    let file = File::open(path).context("Failed to open XISF file")?;
    let mut reader = BufReader::new(file);
    load_xisf_from_reader(&mut reader)
        .with_context(|| format!("Failed to load XISF image from {}", path.display()))
}

fn load_xisf_from_reader<R: Read + Seek>(reader: &mut R) -> Result<(Vec<f32>, usize, usize)> {
    let header = read_monolithic_header(reader)?;
    let image = parse_image_data_block(&header.xml, header.source_len, header.xml_end)?;

    debug!(
        "Parsed XISF image layout: {}x{}, offset={}, size={}",
        image.width, image.height, image.data_offset, image.data_size
    );

    reader
        .seek(SeekFrom::Start(image.data_offset))
        .context("Failed to seek to image data")?;

    let data = if let Some(compression) = &image.compression {
        decode_compressed_image(reader, compression, image.decoding.expected_bytes)?
    } else {
        let mut data = Vec::new();
        data.try_reserve_exact(image.data_size)
            .context("Failed to allocate XISF image data")?;
        data.resize(image.data_size, 0);
        reader
            .read_exact(&mut data)
            .context("Failed to read image data")?;
        data
    };

    let pixels = image.decoding.decode(&data)?;

    Ok((pixels, image.width, image.height))
}

fn decode_compressed_image<R: Read>(
    reader: &mut R,
    compression: &Compression,
    expected_bytes: usize,
) -> Result<Vec<u8>> {
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(expected_bytes)
        .context("Failed to allocate decompressed XISF image data")?;
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(65536)
        .context("Failed to allocate Zstandard output buffer")?;
    scratch.resize(65536, 0);

    for subblock in compression.subblocks() {
        let mut source = ReaderInput::new(reader, subblock.stored)?;
        let requirements = zstd_frame_requirements(
            source
                .fill_buf()
                .context("Failed to read Zstandard frame header")?,
        )
        .map_err(anyhow::Error::new)?;
        if requirements.history_bytes() > MAX_LOADER_ZSTD_HISTORY {
            bail!(
                "Zstandard history requirement {} exceeds loader limit {}",
                requirements.history_bytes(),
                MAX_LOADER_ZSTD_HISTORY
            );
        }
        decode_zstd_frame(
            source,
            subblock.decoded,
            requirements,
            &mut scratch,
            || Ok(()),
            |bytes| {
                decoded.extend_from_slice(bytes);
                Ok(())
            },
        )
        .context("Failed to decode XISF Zstandard subblock")?;
    }
    if decoded.len() != expected_bytes {
        bail!(
            "XISF decompressed image length mismatch: expected exactly {expected_bytes} bytes, got {}",
            decoded.len()
        );
    }
    if let Some(item_size) = compression.shuffle() {
        return unshuffle(
            &decoded,
            usize::try_from(item_size).context("XISF shuffle item size exceeds address space")?,
        )
        .map_err(anyhow::Error::new);
    }
    Ok(decoded)
}

/// Read raw XISF metadata records through the shared structural parser.
///
/// The result contains only format facts used by the semantic metadata crate.
/// Malformed prefixes, ranges, UTF-8, XML, or required metadata-record
/// attributes are errors; they are never converted into an empty successful
/// result.
///
/// # Errors
///
/// Returns an error for I/O failures, an invalid or incomplete monolithic
/// envelope, malformed XML/UTF-8, unsupported namespaces, or a `FITSKeyword`
/// or `Property` missing a required attribute.
pub fn read_metadata_records<R: Read + Seek>(reader: &mut R) -> Result<XisfMetadataRecords> {
    let header = read_monolithic_header(reader)?;
    collect_metadata_records(&header.xml)
}

fn read_monolithic_header<R: Read + Seek>(reader: &mut R) -> Result<MonolithicHeader> {
    let source_len = reader
        .seek(SeekFrom::End(0))
        .context("Failed to determine XISF source extent")?;
    reader
        .seek(SeekFrom::Start(0))
        .context("Failed to seek to XISF prefix")?;
    // A source shorter than the 16-byte prefix passes the available bytes to
    // `MonolithicEnvelope::parse`, which names the specific missing stage.
    // Pre-checking the length here would duplicate that check in a second
    // dialect of the prefix.
    let prefix_len = source_len.min(PREFIX_LEN) as usize;
    let mut prefix = vec![0u8; prefix_len];
    reader
        .read_exact(&mut prefix)
        .context("Failed to read XISF monolithic prefix")?;
    let envelope = MonolithicEnvelope::parse(&prefix, source_len).map_err(anyhow::Error::new)?;
    let range = envelope.xml_range();
    let header_size = usize::try_from(envelope.xml_len())
        .context("XISF XML header exceeds this platform's address space")?;
    let mut header_data = vec![0u8; header_size];
    reader
        .seek(SeekFrom::Start(range.start))
        .context("Failed to seek to XISF XML header")?;
    reader
        .read_exact(&mut header_data)
        .context("Failed to read XML header")?;
    Ok(MonolithicHeader {
        xml: header_data,
        source_len,
        xml_end: envelope.xml_end(),
    })
}

fn collect_metadata_records(xml: &[u8]) -> Result<XisfMetadataRecords> {
    let mut records = XisfMetadataRecords::default();
    let mut pending_property: Option<PendingProperty> = None;

    let result = visit_xml(xml, |event| {
        match event {
            XmlEvent::Element(element) => {
                if !element.namespace.is_core() {
                    return Err(anyhow!("Unsupported XML namespace on {}", element.name));
                }
                if let Some(property) = pending_property.as_mut() {
                    if element.depth > property.depth {
                        property.has_child = true;
                    }
                }
                match element.name.as_str() {
                    "xisf" if element.depth == 0 => {
                        records.version = element.attr("version").map(str::to_string);
                        records.block_alignment =
                            element.attr("blockAlignment").map(str::to_string);
                    }
                    "FITSKeyword" => {
                        let name = required_metadata_attribute(&element, "name")?;
                        let value = required_metadata_attribute(&element, "value")?;
                        records.fits_keywords.push(XisfFitsKeyword {
                            name,
                            value,
                            comment: element.attr("comment").map(str::to_string),
                        });
                    }
                    "Property" => {
                        if pending_property.is_some() {
                            return Err(anyhow!("nested XISF Property elements are malformed"));
                        }
                        let record = XisfProperty {
                            id: required_metadata_attribute(&element, "id")?,
                            declared_type: required_metadata_attribute(&element, "type")?,
                            value: element.attr("value").map(str::to_string),
                        };
                        if element.empty {
                            records.properties.push(record);
                        } else {
                            pending_property = Some(PendingProperty {
                                record,
                                depth: element.depth,
                                text: String::new(),
                                has_child: false,
                            });
                        }
                    }
                    "Image" => records.images.push(image_metadata(&element)),
                    _ => {}
                }
            }
            XmlEvent::Text { value, depth } => {
                if let Some(property) = pending_property.as_mut() {
                    if depth == property.depth + 1 {
                        property.text.push_str(&value);
                    }
                }
            }
            XmlEvent::End { name, depth } => {
                if name == "Property"
                    && pending_property
                        .as_ref()
                        .is_some_and(|property| property.depth == depth)
                {
                    let mut property = pending_property
                        .take()
                        .ok_or_else(|| anyhow!("missing pending XISF Property"))?;
                    if property.record.value.is_some() && !property.text.trim().is_empty() {
                        return Err(anyhow!(
                            "XISF Property cannot have both a value attribute and text"
                        ));
                    }
                    if property.record.value.is_none()
                        && (!property.text.is_empty()
                            && (!property.has_child || !property.text.trim().is_empty()))
                    {
                        property.record.value = Some(property.text);
                    }
                    records.properties.push(property.record);
                }
            }
            XmlEvent::IgnoredExtension { .. } => {}
        }
        Ok(())
    });
    match result {
        Ok(()) => {}
        Err(VisitError::Structural(error)) => return Err(anyhow::Error::new(error)),
        Err(VisitError::Consumer(error)) => return Err(error),
    }
    if pending_property.is_some() {
        bail!("incomplete XISF Property element");
    }
    Ok(records)
}

fn required_metadata_attribute(element: &structural::Element, name: &str) -> Result<String> {
    element
        .attr(name)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{} missing {name}", element.name))
}

fn image_metadata(element: &structural::Element) -> XisfImageMetadata {
    let value = |name| element.attr(name).map(str::to_string);
    XisfImageMetadata {
        id: value("id"),
        geometry: value("geometry"),
        sample_format: value("sampleFormat"),
        color_space: value("colorSpace"),
        bits_per_sample: value("bitsPerSample"),
        compression: value("compression"),
        compression_parameters: value("compressionParameters"),
        checksum_type: value("checksumType"),
        checksum: value("checksum"),
        x_resolution: value("xResolution"),
        y_resolution: value("yResolution"),
        resolution_unit: value("resolutionUnit"),
        display_function: value("displayFunction"),
        display_parameters: value("displayParameters"),
    }
}

fn parse_image_data_block(xml: &[u8], source_len: u64, xml_end: u64) -> Result<ImageDataBlock> {
    let mut descriptor = None;
    match visit_xml(xml, |event| {
        let XmlEvent::Element(element) = event else {
            return Ok(());
        };
        // The loader interprets only the core vocabulary. The shared visitor
        // suppresses legal root-extension subtrees and rejects foreign
        // elements everywhere else before they reach this consumer.
        if !element.namespace.is_core() {
            return Err(anyhow!("Unsupported XML namespace on {}", element.name));
        }
        if element.name == "Image" && descriptor.is_none() {
            descriptor = Some(ImageDescriptor::parse(&element).map_err(anyhow::Error::new)?);
        }
        Ok(())
    }) {
        Ok(()) => {}
        Err(VisitError::Structural(error)) => return Err(anyhow::Error::new(error)),
        Err(VisitError::Consumer(error)) => return Err(error),
    }
    let descriptor = descriptor.context("XISF header is missing an Image element")?;
    let dimensions = descriptor.geometry.dimensions();
    if dimensions.len() != 3 {
        bail!(
            "Invalid XISF geometry; expected width:height:channels, got {} dimensions",
            dimensions.len()
        );
    }
    let (width, height, channels) = (
        usize::try_from(dimensions[0]).context("XISF image width exceeds address space")?,
        usize::try_from(dimensions[1]).context("XISF image height exceeds address space")?,
        dimensions[2],
    );

    let decoding = SampleDecoding::from_descriptor(&descriptor)?;

    if channels != 1 {
        bail!("Unsupported XISF geometry: only single-channel images are currently supported");
    }

    let (data_offset, data_size) = match &descriptor.location {
        BlockLocation::Attachment { offset, size } => {
            // `xml_end` is the single source of the header boundary: the
            // envelope parse checks the declared XML range, not whether an
            // attachment overlaps it, and an attachment starting inside the
            // header would re-read header bytes as payload.
            if *offset < xml_end {
                bail!("XISF attachment overlaps the XML header");
            }
            checked_range(*offset, *size, source_len, "XISF image attachment")
                .map_err(anyhow::Error::new)?;
            let size = usize::try_from(*size)
                .context("XISF image attachment exceeds this platform's address space")?;
            (*offset, size)
        }
        BlockLocation::Inline { .. } => {
            bail!("Unsupported inline XISF image; only attachment-backed images are currently supported")
        }
        BlockLocation::Embedded => {
            bail!("Unsupported embedded XISF image; only attachment-backed images are currently supported")
        }
        BlockLocation::Other(location) => bail!(
            "Unsupported XISF location '{}'; only attachment-backed images are currently supported",
            location
        ),
    };

    let compression = Compression::parse(
        descriptor.compression.as_deref(),
        descriptor.subblocks.as_deref(),
        data_size as u64,
    )
    .map_err(anyhow::Error::new)?;
    if let Some(compression) = &compression {
        if compression.codec() != "zstd" {
            bail!(
                "Unsupported compressed XISF image '{}'; the loader currently supports zstd and zstd+sh",
                compression.codec()
            );
        }
        if compression.decoded() != decoding.expected_bytes as u64 {
            bail!(
                "XISF image geometry requires {} bytes, compression declares {}",
                decoding.expected_bytes,
                compression.decoded()
            );
        }
        if compression
            .shuffle()
            .is_some_and(|item_size| item_size != 2)
        {
            bail!("XISF UInt16 shuffle item size must be 2 bytes");
        }
    } else {
        // An uncompressed attachment extent must match geometry exactly. A
        // compressed extent is instead accounted by its shared descriptor.
        if data_size < decoding.expected_bytes {
            bail!(
                "XISF image payload is truncated: geometry requires {} bytes, location declares {data_size}",
                decoding.expected_bytes
            );
        }
        if data_size > decoding.expected_bytes {
            bail!(
                "XISF image payload has trailing bytes: geometry requires {} bytes, location declares {data_size}",
                decoding.expected_bytes
            );
        }
    }

    Ok(ImageDataBlock {
        width,
        height,
        data_offset,
        data_size,
        decoding,
        compression,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_read_pixel_data() {
        let mut data = Vec::new();
        let pixels = [0u16, 32768u16, 65535u16, 16384u16];

        for pixel in &pixels {
            data.extend_from_slice(&pixel.to_le_bytes());
        }

        let result = uint16_decoding(4, ByteOrder::Little).decode(&data).unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[0], 0.0);
        assert!((result[1] - 0.5).abs() < 0.001);
        assert_eq!(result[2], 1.0);
        assert!((result[3] - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_read_pixel_data_rejects_truncated_buffer() {
        let err = uint16_decoding(4, ByteOrder::Little)
            .decode(&[0u8; 6])
            .expect_err("truncated data should fail");
        assert!(err.to_string().contains("truncated"));
    }

    #[test]
    fn test_read_pixel_data_rejects_trailing_bytes() {
        let err = uint16_decoding(1, ByteOrder::Little)
            .decode(&[0u8; 3])
            .expect_err("trailing data should fail");
        assert!(err.to_string().contains("trailing"));
    }

    #[test]
    fn test_load_xisf_reads_valid_uint16_payload() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#,
            &u16_payload(&[0, 32768, 65535, 16384]),
        );

        let result = load_xisf(&path);
        fs::remove_file(&path).unwrap();

        let (pixels, width, height) = result.expect("valid XISF should load");
        assert_eq!((width, height), (2, 2));
        assert_eq!(pixels.len(), 4);
        assert_eq!(pixels[0], 0.0);
        assert!((pixels[1] - 0.5).abs() < 0.001);
        assert_eq!(pixels[2], 1.0);
        assert!((pixels[3] - 0.25).abs() < 0.001);
    }

    #[test]
    fn load_xisf_decodes_little_and_big_endian_uint16_to_the_same_values() {
        let values = [0x0102, 0xabcd, 0x8001, 0xff00];
        let little = write_temp_xisf(
            r#"<xisf><Image geometry="2:2:1" sampleFormat="UInt16" byteOrder="little" location="attachment:512:8"/></xisf>"#,
            &u16_payload_with_order(&values, "little"),
        );
        let big = write_temp_xisf(
            r#"<xisf><Image geometry="2:2:1" sampleFormat="UInt16" byteOrder="big" location="attachment:512:8"/></xisf>"#,
            &u16_payload_with_order(&values, "big"),
        );

        let little_result = load_xisf(&little);
        let big_result = load_xisf(&big);
        fs::remove_file(&little).unwrap();
        fs::remove_file(&big).unwrap();

        let (little_pixels, little_width, little_height) =
            little_result.expect("little-endian golden image should load");
        let (big_pixels, big_width, big_height) =
            big_result.expect("big-endian golden image should load");
        let expected: Vec<_> = values
            .iter()
            .map(|value| f32::from(*value) / 65535.0)
            .collect();
        assert_eq!((little_width, little_height), (2, 2));
        assert_eq!((big_width, big_height), (2, 2));
        assert_eq!(little_pixels, expected);
        assert_eq!(big_pixels, expected);
    }

    #[test]
    fn load_xisf_defaults_missing_byte_order_to_little_endian() {
        let values = [0x0102, 0xabcd];
        let path = write_temp_xisf(
            r#"<xisf><Image geometry="2:1:1" sampleFormat="UInt16" location="attachment:512:4"/></xisf>"#,
            &u16_payload_with_order(&values, "little"),
        );

        let result = load_xisf(&path);
        fs::remove_file(&path).unwrap();

        let (pixels, width, height) = result.expect("missing byteOrder should default to little");
        assert_eq!((width, height), (2, 1));
        assert_eq!(
            pixels,
            values
                .iter()
                .map(|value| f32::from(*value) / 65535.0)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn load_xisf_rejects_invalid_byte_order() {
        let path = write_temp_xisf(
            r#"<xisf><Image geometry="1:1:1" sampleFormat="UInt16" byteOrder="middle" location="attachment:512:2"/></xisf>"#,
            &[0, 0],
        );

        let error = load_xisf(&path).expect_err("invalid byteOrder must not fall back");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("byteOrder"));
    }

    #[test]
    fn load_xisf_rejects_legacy_twelve_byte_prefix() {
        let xml = r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#;
        let path = temp_xisf_path();
        fs::write(
            &path,
            build_legacy_twelve_byte_xisf(xml, &u16_payload(&[0, 1, 2, 3])),
        )
        .unwrap();

        let error = load_xisf(&path).expect_err("the historical 12-byte form must be rejected");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("reserved"));
    }

    #[test]
    fn load_xisf_rejects_nonzero_reserved_bytes() {
        let xml = r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#;
        let mut bytes = build_xisf_bytes(xml, &u16_payload(&[0, 1, 2, 3]));
        bytes[12] = 1;
        let path = temp_xisf_path();
        fs::write(&path, bytes).unwrap();

        let error = load_xisf(&path).expect_err("reserved prefix bytes must be zero");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("reserved"));
    }

    #[test]
    fn load_xisf_rejects_each_truncated_prefix_section() {
        for (length, expected) in [(7, "signature"), (11, "length"), (15, "reserved")] {
            let path = temp_xisf_path();
            fs::write(&path, &b"XISF0100\x01\0\0\0\0\0\0\0"[..length]).unwrap();

            let error = load_xisf(&path).expect_err("truncated prefix must fail");
            fs::remove_file(&path).unwrap();

            assert!(
                format!("{error:#}").contains(expected),
                "length {length} should identify the truncated {expected} field: {error:#}"
            );
        }
    }

    #[test]
    fn load_xisf_rejects_invalid_utf8_and_malformed_xml() {
        let mut invalid_utf8 = build_xisf_bytes(
            r#"<xisf><Image geometry="1:1:1" sampleFormat="UInt16" location="attachment:512:2"/></xisf>"#,
            &[0, 0],
        );
        invalid_utf8[16] = 0xff;
        let invalid_utf8_path = temp_xisf_path();
        fs::write(&invalid_utf8_path, invalid_utf8).unwrap();
        let error = load_xisf(&invalid_utf8_path).expect_err("invalid UTF-8 must fail");
        fs::remove_file(&invalid_utf8_path).unwrap();
        assert!(format!("{error:#}").contains("UTF-8"));

        let malformed_path = write_temp_xisf(
            r#"<xisf><Image geometry="1:1:1" sampleFormat="UInt16" location="attachment:512:2"/>"#,
            &[0, 0],
        );
        let error = load_xisf(&malformed_path).expect_err("malformed XML must fail");
        fs::remove_file(&malformed_path).unwrap();
        assert!(format!("{error:#}").contains("XML"));
    }

    #[test]
    fn load_xisf_accepts_qualified_core_namespace_elements() {
        let path = write_temp_xisf(
            r#"<x:xisf xmlns:x="http://www.pixinsight.com/xisf"><x:Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8"/></x:xisf>"#,
            &u16_payload(&[0, 32768, 65535, 16384]),
        );

        let result = load_xisf(&path);
        fs::remove_file(&path).unwrap();

        let (pixels, width, height) = result.expect("qualified core elements should load");
        assert_eq!((width, height, pixels.len()), (2, 2, 4));
    }

    #[test]
    fn load_xisf_decodes_zstd_uint16_in_both_byte_orders() {
        let values = [0x0102, 0xabcd, 0x8001, 0xff00];
        for byte_order in ["little", "big"] {
            let raw = u16_payload_with_order(&values, byte_order);
            let compressed = zstd_frame(&raw);
            let xml = format!(
                r#"<xisf><Image geometry="2:2:1" sampleFormat="UInt16" byteOrder="{byte_order}" compression="zstd:8" location="attachment:512:{}"/></xisf>"#,
                compressed.len()
            );
            let path = write_temp_xisf(&xml, &compressed);

            let result = load_xisf(&path);
            fs::remove_file(&path).unwrap();

            let (pixels, width, height) = result.expect("valid zstd image should load");
            assert_eq!((width, height), (2, 2));
            assert_eq!(
                pixels,
                values
                    .iter()
                    .map(|value| f32::from(*value) / 65535.0)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn load_xisf_decodes_whole_block_shuffle_after_multiple_zstd_subblocks() {
        let values = [0x0102, 0x0304, 0xa0b0, 0xc0d0];
        let raw = u16_payload(&values);
        let shuffled = shuffle(&raw, 2);
        // Split across byte planes. Unshuffling either subblock separately
        // would produce different pixels and fail this golden assertion.
        let first = zstd_frame(&shuffled[..3]);
        let second = zstd_frame(&shuffled[3..]);
        let mut compressed = first.clone();
        compressed.extend_from_slice(&second);
        let xml = format!(
            r#"<xisf><Image geometry="4:1:1" sampleFormat="UInt16" compression="zstd+sh:8:2" subblocks="{},3:{},5" location="attachment:512:{}"/></xisf>"#,
            first.len(),
            second.len(),
            compressed.len()
        );
        let path = write_temp_xisf(&xml, &compressed);

        let result = load_xisf(&path);
        fs::remove_file(&path).unwrap();

        let (pixels, width, height) = result.expect("multi-subblock zstd+sh should load");
        assert_eq!((width, height), (4, 1));
        assert_eq!(
            pixels,
            values
                .iter()
                .map(|value| f32::from(*value) / 65535.0)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn load_xisf_rejects_zstd_size_frame_and_shuffle_violations() {
        let raw = u16_payload(&[1, 2, 3, 4]);
        let frame = zstd_frame(&raw);
        let mut concatenated = frame.clone();
        concatenated.extend_from_slice(&frame);
        let mut trailing = frame.clone();
        trailing.extend_from_slice(&[1, 2, 3, 4]);
        let mut skippable = 0x184d2a50u32.to_le_bytes().to_vec();
        skippable.extend_from_slice(&3u32.to_le_bytes());
        skippable.extend_from_slice(&[1, 2, 3]);
        skippable.extend_from_slice(&frame);
        let mut trailing_skippable = frame.clone();
        trailing_skippable.extend_from_slice(&0x184d2a50u32.to_le_bytes());
        trailing_skippable.extend_from_slice(&3u32.to_le_bytes());
        trailing_skippable.extend_from_slice(&[1, 2, 3]);
        let truncated = frame[..frame.len() - 1].to_vec();

        for (name, payload) in [
            ("concatenated", concatenated),
            ("trailing", trailing),
            ("skippable", skippable),
            ("trailing skippable", trailing_skippable),
            ("truncated", truncated),
        ] {
            let xml = format!(
                r#"<xisf><Image geometry="4:1:1" sampleFormat="UInt16" compression="zstd:8" location="attachment:512:{}"/></xisf>"#,
                payload.len()
            );
            let path = write_temp_xisf(&xml, &payload);
            let error = load_xisf(&path).expect_err(name);
            fs::remove_file(&path).unwrap();
            assert!(
                format!("{error:#}").contains("Zstandard"),
                "{name}: {error:#}"
            );
        }

        // A valid-looking frame header that advertises a multi-terabyte
        // window must be rejected before the native decoder can reserve it.
        let excessive_history = [0x28, 0xb5, 0x2f, 0xfd, 0, 0xf8];
        let xml = format!(
            r#"<xisf><Image geometry="4:1:1" sampleFormat="UInt16" compression="zstd:8" location="attachment:512:{}"/></xisf>"#,
            excessive_history.len()
        );
        let path = write_temp_xisf(&xml, &excessive_history);
        let error = load_xisf(&path).expect_err("excessive Zstandard history must fail");
        fs::remove_file(&path).unwrap();
        assert!(format!("{error:#}").contains("history requirement"));

        for (name, decoded) in [("short", vec![0; 6]), ("long", vec![0; 10])] {
            let payload = zstd_frame(&decoded);
            let xml = format!(
                r#"<xisf><Image geometry="4:1:1" sampleFormat="UInt16" compression="zstd:8" location="attachment:512:{}"/></xisf>"#,
                payload.len()
            );
            let path = write_temp_xisf(&xml, &payload);
            let error = load_xisf(&path).expect_err(name);
            fs::remove_file(&path).unwrap();
            assert!(
                format!("{error:#}").contains("Zstandard"),
                "{name}: {error:#}"
            );
        }

        for declared in [6, 10] {
            let xml = format!(
                r#"<xisf><Image geometry="4:1:1" sampleFormat="UInt16" compression="zstd:{declared}" location="attachment:512:{}"/></xisf>"#,
                frame.len()
            );
            let path = write_temp_xisf(&xml, &frame);
            let error = load_xisf(&path).expect_err("decoded size mismatch must fail");
            fs::remove_file(&path).unwrap();
            assert!(format!("{error:#}").contains("geometry"), "{error:#}");
        }

        let xml = format!(
            r#"<xisf><Image geometry="4:1:1" sampleFormat="UInt16" compression="zstd+sh:8:4" location="attachment:512:{}"/></xisf>"#,
            frame.len()
        );
        let path = write_temp_xisf(&xml, &frame);
        let error = load_xisf(&path).expect_err("UInt16 shuffle width must be two");
        fs::remove_file(&path).unwrap();
        assert!(format!("{error:#}").contains("shuffle item size"));
    }

    #[test]
    fn root_foreign_extensions_are_ignored_but_nested_foreign_elements_fail() {
        let accepted = write_temp_xisf(
            r#"<xisf xmlns:e="urn:example"><e:Extension><e:Nested><Image/></e:Nested></e:Extension><Image geometry="2:1:1" sampleFormat="UInt16" location="attachment:512:4"/></xisf>"#,
            &u16_payload(&[1, 2]),
        );
        let result = load_xisf(&accepted);
        fs::remove_file(&accepted).unwrap();
        assert!(
            result.is_ok(),
            "root extension should be ignored: {result:?}"
        );

        let nested = write_temp_xisf(
            r#"<xisf xmlns:e="urn:example"><Image geometry="2:1:1" sampleFormat="UInt16" location="attachment:512:4"><e:Extension/></Image></xisf>"#,
            &u16_payload(&[1, 2]),
        );
        let error = load_xisf(&nested).expect_err("nested foreign extension must fail");
        fs::remove_file(&nested).unwrap();
        assert!(format!("{error:#}").contains("namespace"));

        let records = build_metadata_xisf(
            r#"<xisf xmlns:e="urn:example"><e:Extension><Property id="Hidden" type="String" value="no"/></e:Extension><Property id="Visible" type="String" value="yes"/></xisf>"#,
        );
        let records = read_metadata_records(&mut std::io::Cursor::new(records))
            .expect("metadata reader should ignore root extension subtrees");
        assert_eq!(records.properties().len(), 1);
        assert_eq!(records.properties()[0].id(), "Visible");
    }

    #[test]
    fn test_load_xisf_rejects_missing_geometry() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:8" /></xisf>"#,
            &u16_payload(&[0, 1, 2, 3]),
        );

        let err = load_xisf(&path).expect_err("missing geometry should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("geometry"));
    }

    #[test]
    fn test_load_xisf_rejects_missing_location() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" /></xisf>"#,
            &u16_payload(&[0, 1, 2, 3]),
        );

        let err = load_xisf(&path).expect_err("missing location should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("location"));
    }

    #[test]
    fn test_load_xisf_rejects_truncated_payload() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="UInt16" colorSpace="Gray" location="attachment:512:6" /></xisf>"#,
            &[0u8; 6],
        );

        let err = load_xisf(&path).expect_err("truncated payload should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("truncated"));
    }

    #[test]
    fn load_xisf_rejects_payload_one_byte_short_and_incomplete_final_sample() {
        let path = write_temp_xisf(
            r#"<xisf><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:512:7"/></xisf>"#,
            &[0u8; 7],
        );

        let error = load_xisf(&path).expect_err("an incomplete final sample must fail");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("truncated"));
    }

    #[test]
    fn load_xisf_rejects_declared_payload_with_trailing_bytes() {
        let path = write_temp_xisf(
            r#"<xisf><Image geometry="2:2:1" sampleFormat="UInt16" location="attachment:512:9"/></xisf>"#,
            &[0u8; 9],
        );

        let error = load_xisf(&path).expect_err("declared trailing payload bytes must fail");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("trailing"));
    }

    #[test]
    fn load_xisf_allows_bytes_outside_the_declared_attachment_extent() {
        let values = [0x0102, 0xabcd];
        let mut payload = u16_payload(&values);
        payload.extend_from_slice(&[0xde, 0xad]);
        let path = write_temp_xisf(
            r#"<xisf><Image geometry="2:1:1" sampleFormat="UInt16" location="attachment:512:4"/></xisf>"#,
            &payload,
        );

        let result = load_xisf(&path);
        fs::remove_file(&path).unwrap();

        let (pixels, width, height) =
            result.expect("bytes outside the declared block belong to other file content");
        assert_eq!((width, height), (2, 1));
        assert_eq!(
            pixels,
            values
                .iter()
                .map(|value| f32::from(*value) / 65535.0)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn load_xisf_rejects_image_sample_count_overflow() {
        let path = write_temp_xisf(
            r#"<xisf><Image geometry="18446744073709551615:2:1" sampleFormat="UInt16" location="attachment:512:2"/></xisf>"#,
            &[0u8; 2],
        );

        let error = load_xisf(&path).expect_err("sample-count overflow must fail");
        fs::remove_file(&path).unwrap();

        assert!(format!("{error:#}").contains("sample count overflows"));
    }

    #[test]
    fn metadata_records_are_empty_when_the_document_has_no_relevant_elements() {
        let bytes = build_metadata_xisf(r#"<xisf version="1.0"><Metadata/></xisf>"#);

        let records = read_metadata_records(&mut std::io::Cursor::new(bytes)).unwrap();

        assert_eq!(records.version(), Some("1.0"));
        assert_eq!(records.block_alignment(), None);
        assert!(records.fits_keywords().is_empty());
        assert!(records.properties().is_empty());
        assert!(records.images().is_empty());
    }

    #[test]
    fn xml_header_preserves_unknown_content_and_rejects_invalid_utf8() {
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<xisf xmlns:writer="urn:writer" version="1.0">"#,
            r#"<!-- writer-provided diagnostic -->"#,
            r#"<writer:Unknown priority="high">retain this text</writer:Unknown>"#,
            r#"</xisf>"#,
        );
        let bytes = build_metadata_xisf(xml);

        let header = read_xisf_xml_header(&mut std::io::Cursor::new(bytes))
            .expect("the shared envelope path should return the complete XML header");
        assert_eq!(header, xml);

        let mut invalid_utf8 = build_metadata_xisf("<xisf/>");
        let last = invalid_utf8.len() - 1;
        invalid_utf8[last] = 0xff;
        let error = read_xisf_xml_header(&mut std::io::Cursor::new(invalid_utf8))
            .expect_err("a non-UTF-8 XML header must be rejected");
        assert!(format!("{error:#}").contains("UTF-8"));

        let malformed = build_metadata_xisf("<xisf>");
        let error = read_xisf_xml_header(&mut std::io::Cursor::new(malformed))
            .expect_err("a malformed XML header must be rejected");
        assert!(format!("{error:#}").contains("XML"));
    }

    #[test]
    fn metadata_records_preserve_order_duplicates_entities_and_raw_values() {
        let xml = concat!(
            r#"<xisf version="1.0" blockAlignment="4096">"#,
            r#"<Property id="XISF:CreatorApplication" type="String">  PixInsight &amp; Co  </Property>"#,
            r#"<Image id="main" geometry="2:3:1" sampleFormat="UInt16" colorSpace="Gray" bitsPerSample="16" compression="zlib:6" compressionParameters="itemSize=2;level=6" checksumType="sha256" checksum="abcd" xResolution="72" yResolution="73" resolutionUnit="inch" displayFunction="STF" displayParameters="m=0.5;s=1">"#,
            r#"<FITSKeyword name="OBJECT" value="  &apos;M31 &amp; M32   &apos;  " comment="A &amp; B"/>"#,
            r#"<FITSKeyword name="OBJECT" value="M33"/>"#,
            r#"</Image>"#,
            r#"<Property id="Example:Scalar" type="String" value="A &lt; B"/>"#,
            r#"<Property id="Example:Absent" type="String"/>"#,
            r#"<Image/>"#,
            r#"</xisf>"#,
        );
        let bytes = build_metadata_xisf(xml);

        let records = read_metadata_records(&mut std::io::Cursor::new(bytes)).unwrap();

        assert_eq!(records.version(), Some("1.0"));
        assert_eq!(records.block_alignment(), Some("4096"));
        assert_eq!(records.fits_keywords().len(), 2);
        assert_eq!(records.fits_keywords()[0].name(), "OBJECT");
        assert_eq!(records.fits_keywords()[0].value(), "  'M31 & M32   '  ");
        assert_eq!(records.fits_keywords()[0].comment(), Some("A & B"));
        assert_eq!(records.fits_keywords()[1].name(), "OBJECT");
        assert_eq!(records.fits_keywords()[1].value(), "M33");
        assert_eq!(records.fits_keywords()[1].comment(), None);

        assert_eq!(records.properties().len(), 3);
        assert_eq!(records.properties()[0].id(), "XISF:CreatorApplication");
        assert_eq!(records.properties()[0].declared_type(), "String");
        assert_eq!(records.properties()[0].value(), Some("  PixInsight & Co  "));
        assert_eq!(records.properties()[1].id(), "Example:Scalar");
        assert_eq!(records.properties()[1].value(), Some("A < B"));
        assert_eq!(records.properties()[2].id(), "Example:Absent");
        assert_eq!(records.properties()[2].value(), None);

        let image = &records.images()[0];
        assert_eq!(image.id(), Some("main"));
        assert_eq!(image.geometry(), Some("2:3:1"));
        assert_eq!(image.sample_format(), Some("UInt16"));
        assert_eq!(image.color_space(), Some("Gray"));
        assert_eq!(image.bits_per_sample(), Some("16"));
        assert_eq!(image.compression(), Some("zlib:6"));
        assert_eq!(image.compression_parameters(), Some("itemSize=2;level=6"));
        assert_eq!(image.checksum_type(), Some("sha256"));
        assert_eq!(image.checksum(), Some("abcd"));
        assert_eq!(image.x_resolution(), Some("72"));
        assert_eq!(image.y_resolution(), Some("73"));
        assert_eq!(image.resolution_unit(), Some("inch"));
        assert_eq!(image.display_function(), Some("STF"));
        assert_eq!(image.display_parameters(), Some("m=0.5;s=1"));

        let optional = &records.images()[1];
        assert_eq!(optional.id(), None);
        assert_eq!(optional.geometry(), None);
        assert_eq!(optional.sample_format(), None);
        assert_eq!(optional.color_space(), None);
        assert_eq!(optional.compression(), None);
        assert_eq!(optional.checksum(), None);
        assert_eq!(optional.display_function(), None);
    }

    #[test]
    fn metadata_records_reject_malformed_records_xml_and_utf8() {
        let missing_value =
            build_metadata_xisf(r#"<xisf><Image><FITSKeyword name="OBJECT"/></Image></xisf>"#);
        let error = read_metadata_records(&mut std::io::Cursor::new(missing_value))
            .expect_err("missing FITSKeyword value must fail");
        assert!(format!("{error:#}").contains("FITSKeyword missing value"));

        let missing_property_type =
            build_metadata_xisf(r#"<xisf><Property id="Example:Value"/></xisf>"#);
        let error = read_metadata_records(&mut std::io::Cursor::new(missing_property_type))
            .expect_err("missing Property type must fail");
        assert!(format!("{error:#}").contains("Property missing type"));

        let malformed = build_metadata_xisf(r#"<xisf><Property id="A" type="String">"#);
        let error = read_metadata_records(&mut std::io::Cursor::new(malformed))
            .expect_err("malformed XML must fail");
        assert!(format!("{error:#}").contains("XML"));

        let mut invalid_utf8 = build_metadata_xisf("<xisf/> ");
        let last = invalid_utf8.len() - 1;
        invalid_utf8[last] = 0xff;
        let error = read_metadata_records(&mut std::io::Cursor::new(invalid_utf8))
            .expect_err("invalid UTF-8 must fail");
        assert!(format!("{error:#}").contains("UTF-8"));
    }

    #[test]
    fn test_load_xisf_rejects_unsupported_sample_format() {
        let path = write_temp_xisf(
            r#"<?xml version="1.0"?><xisf><Image geometry="2:2:1" sampleFormat="Float32" colorSpace="Gray" location="attachment:512:16" /></xisf>"#,
            &[0u8; 16],
        );

        let err = load_xisf(&path).expect_err("unsupported sample format should fail");
        fs::remove_file(&path).unwrap();

        let message = format!("{err:#}");
        assert!(message.contains("sampleFormat"));
    }

    #[test]
    #[ignore = "requires the local capture in tests/data; run explicitly where available"]
    fn test_load_xisf_real_sample() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/data/2024-08-23_21-44-23_LIGHT_-10.00_60.00s_0000_a.xisf");

        let (pixels, width, height) = load_xisf(&path).expect("sample XISF should load");

        assert_eq!((width, height), (3856, 2180));
        assert_eq!(pixels.len(), width * height);
    }

    fn write_temp_xisf(xml: &str, payload: &[u8]) -> PathBuf {
        let path = temp_xisf_path();
        let bytes = build_xisf_bytes(xml, payload);
        fs::write(&path, bytes).unwrap();
        path
    }

    fn temp_xisf_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ravensky-astro-xisf-{}-{}.xisf",
            process::id(),
            unique
        ))
    }

    /// Well-formed fixture with the full 16-byte prefix. The XML header is
    /// zero-padded so the payload always begins at the fixed 512-byte
    /// `attachment` offset the XML declares.
    fn build_xisf_bytes(xml: &str, payload: &[u8]) -> Vec<u8> {
        const DATA_OFFSET: usize = 512;
        const PREFIX_LEN: usize = 16;

        assert!(PREFIX_LEN + xml.len() <= DATA_OFFSET);

        let mut bytes = Vec::with_capacity(DATA_OFFSET + payload.len());
        bytes.extend_from_slice(b"XISF0100");
        bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(xml.as_bytes());
        bytes.resize(DATA_OFFSET, 0);
        bytes.extend_from_slice(payload);

        bytes
    }

    fn build_metadata_xisf(xml: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16 + xml.len());
        bytes.extend_from_slice(b"XISF0100");
        bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(xml.as_bytes());
        bytes
    }

    /// The defective historical form: XML starting immediately after the
    /// 8+4-byte signature/length with the four reserved bytes consumed as
    /// XML. A negative fixture — the 12-byte prefix is a format defect, and
    /// a loader that accepts it would be wrong, not more compatible.
    fn build_legacy_twelve_byte_xisf(xml: &str, payload: &[u8]) -> Vec<u8> {
        const DATA_OFFSET: usize = 512;
        let mut bytes = Vec::with_capacity(DATA_OFFSET + payload.len());
        bytes.extend_from_slice(b"XISF0100");
        bytes.extend_from_slice(&(xml.len() as u32).to_le_bytes());
        bytes.extend_from_slice(xml.as_bytes());
        bytes.resize(DATA_OFFSET, 0);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn u16_payload(values: &[u16]) -> Vec<u8> {
        u16_payload_with_order(values, "little")
    }

    fn u16_payload_with_order(values: &[u16], byte_order: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(values.len() * 2);
        for value in values {
            let encoded = match byte_order {
                "little" => value.to_le_bytes(),
                "big" => value.to_be_bytes(),
                _ => panic!("test helper requires a supported byte order"),
            };
            bytes.extend_from_slice(&encoded);
        }
        bytes
    }

    fn zstd_frame(data: &[u8]) -> Vec<u8> {
        let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 1).unwrap();
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn shuffle(data: &[u8], item_size: usize) -> Vec<u8> {
        let items = data.len() / item_size;
        let complete = items * item_size;
        let mut shuffled = Vec::with_capacity(data.len());
        for byte in 0..item_size {
            for item in 0..items {
                shuffled.push(data[item * item_size + byte]);
            }
        }
        shuffled.extend_from_slice(&data[complete..]);
        shuffled
    }

    fn uint16_decoding(sample_count: usize, byte_order: ByteOrder) -> SampleDecoding {
        SampleDecoding {
            decoder: SampleDecoder::UInt16(byte_order),
            sample_count,
            expected_bytes: sample_count * 2,
        }
    }
}
