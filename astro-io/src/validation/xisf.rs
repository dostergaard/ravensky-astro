use super::*;
use crate::xisf::structural::{
    visit_xml, BlockLocation, Error as StructuralError, ErrorKind as StructuralErrorKind,
    ImageDescriptor, MonolithicEnvelope, VisitError, XmlEvent, PREFIX_LEN,
};
use base64::Engine;
use sha2::Digest;
use std::{collections::BTreeMap, io::BufRead};
mod streaming;

// Validation keeps topology and accumulated text for conformance/checksum
// policy. Syntax, namespaces, attributes, and image/storage descriptors come
// from `xisf::structural`; this tree is deliberately not a shared XISF AST.
#[derive(Default)]
struct Node {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<usize>,
    parent: Option<usize>,
    image: Option<ImageDescriptor>,
}
fn natural(s: &str) -> Result<u64> {
    s.parse()
        .map_err(|_| invalid(format!("invalid unsigned integer '{s}'")))
}
fn attr<'a>(n: &'a Node, key: &str) -> Result<&'a str> {
    n.attrs
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| invalid(format!("{} missing {key}", n.name)))
}
fn structural(error: StructuralError) -> ValidationError {
    match error.kind() {
        StructuralErrorKind::Incomplete => {
            ValidationError::new(ValidationErrorKind::Incomplete, error.to_string())
        }
        StructuralErrorKind::Invalid => invalid(error.to_string()),
        StructuralErrorKind::Unsupported => unsupported(error.to_string()),
    }
}
fn parse(c: &mut Context<'_>, xml: &[u8], metadata: &mut Reservation) -> Result<Vec<Node>> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack = Vec::new();
    let result = visit_xml(xml, |event| {
        c.checkpoint()?;
        match event {
            XmlEvent::Element(element) => {
                c.structure()?;
                metadata.grow(1024)?;
                // Namespace-less headers are accepted for compatibility with
                // common producers and existing fixtures. Other namespaces are
                // not interpreted as XISF layout elements.
                if !element.namespace.is_core() {
                    return Err(unsupported(format!("XML namespace on {}", element.name)));
                }
                if element.depth != stack.len() {
                    return Err(invalid("inconsistent XML element depth"));
                }
                for _ in &element.attrs {
                    metadata.grow(1024)?;
                }
                let image = if matches!(element.name.as_str(), "Image" | "Thumbnail") {
                    Some(ImageDescriptor::parse(&element).map_err(structural)?)
                } else {
                    None
                };
                let node = Node {
                    name: element.name,
                    attrs: element.attrs,
                    parent: stack.last().copied(),
                    image,
                    ..Node::default()
                };
                let idx = nodes.len();
                if let Some(&parent) = stack.last() {
                    let p: &mut Node = &mut nodes[parent];
                    p.children
                        .try_reserve(1)
                        .map_err(|_| limit("XML children allocation failed"))?;
                    p.children.push(idx);
                }
                nodes
                    .try_reserve(1)
                    .map_err(|_| limit("XML node allocation failed"))?;
                nodes.push(node);
                if !element.empty {
                    stack
                        .try_reserve(1)
                        .map_err(|_| limit("XML depth allocation failed"))?;
                    stack.push(idx);
                }
            }
            XmlEvent::End { name, depth } => {
                let Some(idx) = stack.pop() else {
                    return Err(invalid("unexpected XML closing element"));
                };
                if nodes[idx].name != name || stack.len() != depth {
                    return Err(invalid("inconsistent XML closing element"));
                }
            }
            XmlEvent::Text { value, depth } => {
                if depth != stack.len() {
                    return Err(invalid("inconsistent XML text depth"));
                }
                if let Some(&idx) = stack.last() {
                    nodes[idx]
                        .text
                        .try_reserve(value.len())
                        .map_err(|_| limit("XML text allocation failed"))?;
                    nodes[idx].text.push_str(&value);
                }
            }
        }
        Ok(())
    });
    match result {
        Ok(()) => {}
        Err(VisitError::Structural(error)) => return Err(structural(error)),
        Err(VisitError::Consumer(error)) => return Err(error),
    }
    if attr(&nodes[0], "version")? != "1.0" {
        return Err(unsupported("XISF version other than 1.0"));
    }
    Ok(nodes)
}
fn expected(n: &Node) -> Result<Option<u64>> {
    if n.name == "Image" || n.name == "Thumbnail" {
        let descriptor = n
            .image
            .as_ref()
            .ok_or_else(|| invalid("missing shared image descriptor"))?;
        return descriptor.expected_bytes().map(Some).map_err(structural);
    }
    if n.name == "Property" {
        let typ = attr(n, "type")?;
        let scalar = match typ.trim_end_matches("Vector").trim_end_matches("Matrix") {
            "I8" | "UI8" | "ByteArray" => Some(1),
            "I16" | "UI16" => Some(2),
            "I32" | "UI32" | "F32" => Some(4),
            "I64" | "UI64" | "F64" | "C32" => Some(8),
            "C64" => Some(16),
            _ => None,
        };
        if let Some(width) = scalar {
            let count = if typ.ends_with("Matrix") {
                mul(natural(attr(n, "rows")?)?, natural(attr(n, "columns")?)?)?
            } else {
                natural(attr(n, "length")?)?
            };
            return Ok(Some(mul(width, count)?));
        }
    }
    Ok(None)
}
struct Compression {
    codec: String,
    decoded: u64,
    shuffle: Option<u64>,
    parts: Vec<(u64, u64)>,
}
fn compression(c: &mut Context<'_>, n: &Node, stored: u64) -> Result<Option<Compression>> {
    let Some(value) = n.attrs.get("compression") else {
        if n.attrs.contains_key("subblocks") {
            return Err(invalid("subblocks without compression"));
        }
        return Ok(None);
    };
    let pieces: Vec<_> = value.split(':').collect();
    if !(2..=3).contains(&pieces.len()) {
        return Err(invalid("invalid compression descriptor"));
    }
    let shuffle = if pieces[0].ends_with("+sh") {
        let size = natural(
            pieces
                .get(2)
                .ok_or_else(|| invalid("missing shuffle item size"))?,
        )?;
        if size == 0 {
            return Err(invalid("zero shuffle item size"));
        }
        Some(size)
    } else {
        if pieces.len() != 2 {
            return Err(invalid("unexpected compression parameter"));
        }
        None
    };
    let codec = pieces[0].trim_end_matches("+sh").to_string();
    let decoded = natural(pieces[1])?;
    let mut parts = Vec::new();
    if let Some(value) = n.attrs.get("subblocks") {
        for p in value.split(':') {
            c.structure()?;
            let (a, b) = p
                .split_once(',')
                .ok_or_else(|| invalid("invalid compression subblock"))?;
            let (a, b) = (natural(a)?, natural(b)?);
            if a == 0 || b == 0 {
                return Err(invalid("empty compression subblock"));
            }
            parts
                .try_reserve(1)
                .map_err(|_| limit("subblock descriptor allocation failed"))?;
            parts.push((a, b));
        }
    } else {
        parts.push((stored, decoded));
    }
    let (mut input, mut output) = (0, 0);
    for &(a, b) in &parts {
        input = add(input, a)?;
        output = add(output, b)?;
    }
    if input != stored || output != decoded {
        return Err(invalid("subblocks do not match stored/decoded lengths"));
    }
    if let Some(size) = shuffle {
        if size > decoded {
            return Err(invalid("shuffle item size exceeds payload"));
        }
    }
    Ok(Some(Compression {
        codec,
        decoded,
        shuffle,
        parts,
    }))
}
enum Storage {
    Attached(u64, u64),
    Inline(Buffer),
}
impl Storage {
    fn input<'a, 'ctx>(
        &'a self,
        c: &'a mut Context<'ctx>,
        offset: u64,
        length: u64,
    ) -> Result<super::input::Input<'a, 'ctx>> {
        if add(offset, length)? > self.len() {
            return Err(invalid("subblock exceeds stored payload"));
        }
        match self {
            Self::Attached(base, _) => {
                super::input::Input::attached(c, add(*base, offset)?, length)
            }
            Self::Inline(bytes) => {
                super::input::Input::inline(c, &bytes[offset as usize..(offset + length) as usize])
            }
        }
    }
    fn len(&self) -> u64 {
        match self {
            Self::Attached(_, n) => *n,
            Self::Inline(b) => b.len() as u64,
        }
    }
    fn stream(&self, c: &mut Context<'_>, consume: impl FnMut(&[u8])) -> Result<()> {
        match self {
            Self::Attached(o, n) => c.stream(*o, *n, consume),
            Self::Inline(b) => {
                let mut consume = consume;
                for chunk in b.chunks(65536) {
                    c.checkpoint()?;
                    consume(chunk);
                }
                Ok(())
            }
        }
    }
    fn part<'a>(&'a self, c: &mut Context<'_>, offset: u64, size: u64) -> Result<Block<'a>> {
        match self {
            Self::Attached(o, _) => c.bytes(add(*o, offset)?, size).map(Block::Owned),
            Self::Inline(b) => {
                let start =
                    usize::try_from(offset).map_err(|_| limit("offset exceeds address space"))?;
                let end = usize::try_from(add(offset, size)?)
                    .map_err(|_| limit("offset exceeds address space"))?;
                Ok(Block::Borrowed(b.get(start..end).ok_or_else(|| {
                    invalid("subblock exceeds inline payload")
                })?))
            }
        }
    }
}
enum Block<'a> {
    Owned(Buffer),
    Borrowed(&'a [u8]),
}
impl std::ops::Deref for Block<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Owned(b) => b,
            Self::Borrowed(b) => b,
        }
    }
}
fn inline(c: &Context<'_>, node: &Node, encoding: &str) -> Result<Buffer> {
    if !node.children.is_empty() {
        return Err(invalid("inline Data has child elements"));
    }
    let mut packed = c.memory.buffer(node.text.len())?;
    let mut length = 0;
    for b in node.text.bytes().filter(|b| !b.is_ascii_whitespace()) {
        packed[length] = b;
        length += 1;
    }
    let text = &packed[..length];
    match encoding {
        "base64" => {
            let mut output = c.memory.buffer((length / 4 + 1) * 3)?;
            let n = base64::engine::general_purpose::STANDARD
                .decode_slice(text, &mut output)
                .map_err(|_| invalid("invalid Base64 block"))?;
            output.truncate(n);
            Ok(output)
        }
        "hex" => {
            if !length.is_multiple_of(2) {
                return Err(invalid("odd hexadecimal length"));
            }
            let mut output = c.memory.buffer(length / 2)?;
            for (out, pair) in output.iter_mut().zip(text.as_chunks::<2>().0) {
                *out = hex_pair(pair)?;
            }
            Ok(output)
        }
        _ => Err(unsupported(format!("inline encoding {encoding}"))),
    }
}
fn hex_pair(p: &[u8]) -> Result<u8> {
    let a = (p[0] as char)
        .to_digit(16)
        .ok_or_else(|| invalid("invalid hexadecimal digit"))?;
    let b = (p[1] as char)
        .to_digit(16)
        .ok_or_else(|| invalid("invalid hexadecimal digit"))?;
    Ok((a * 16 + b) as u8)
}
fn hex(bytes: &[u8]) -> Result<Vec<u8>> {
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid("odd hexadecimal length"));
    }
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            let a = (p[0] as char)
                .to_digit(16)
                .ok_or_else(|| invalid("invalid hexadecimal digit"))?;
            let b = (p[1] as char)
                .to_digit(16)
                .ok_or_else(|| invalid("invalid hexadecimal digit"))?;
            Ok((a * 16 + b) as u8)
        })
        .collect()
}
fn hash<D: Digest + Default>(storage: &Storage, c: &mut Context<'_>) -> Result<Vec<u8>> {
    let mut d = D::default();
    storage.stream(c, |b| d.update(b))?;
    Ok(d.finalize().to_vec())
}
fn checksum(c: &mut Context<'_>, n: &Node, s: &Storage) -> Result<()> {
    let Some(value) = n.attrs.get("checksum") else {
        return Ok(());
    };
    c.checksums.present += 1;
    let (algorithm, digest) = value
        .split_once(':')
        .ok_or_else(|| invalid("invalid checksum descriptor"))?;
    if digest.len() > 256 {
        return Err(invalid("checksum digest too long"));
    }
    let expected = hex(digest.as_bytes())?;
    if c.options.level == ValidationLevel::Structural {
        return Ok(());
    }
    let actual = match algorithm {
        "sha1" | "sha-1" => hash::<sha1::Sha1>(s, c)?,
        "sha256" | "sha-256" => hash::<sha2::Sha256>(s, c)?,
        "sha512" | "sha-512" => hash::<sha2::Sha512>(s, c)?,
        "sha3-256" => hash::<sha3::Sha3_256>(s, c)?,
        "sha3-512" => hash::<sha3::Sha3_512>(s, c)?,
        _ => return Err(unsupported(format!("checksum algorithm {algorithm}"))),
    };
    if actual != expected {
        return Err(integrity("XISF checksum mismatch"));
    }
    c.checksums.verified += 1;
    Ok(())
}
pub(super) fn validate(c: &mut Context<'_>) -> Result<()> {
    let prefix_len = c.stamp.size.min(PREFIX_LEN);
    let prefix = c.bytes(0, prefix_len)?;
    let envelope = MonolithicEnvelope::parse(&prefix, c.stamp.size).map_err(structural)?;
    drop(prefix);
    let size = envelope.xml_len();
    c.header(size)?;
    // Account parser scratch, namespace strings, owned text and reallocation
    // overlap before parsing; per-node/attribute metadata is admitted separately.
    let mut metadata = c.memory.reserve(mul(size, 32)?)?;
    let xml = c.bytes(envelope.xml_range().start, size)?;
    let nodes = parse(c, &xml, &mut metadata)?;
    drop(xml);
    let mut identifiers = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        if let Some(uid) = node.attrs.get("uid") {
            if uid.is_empty()
                || node.name == "Reference"
                || identifiers.insert(uid.as_str(), index).is_some()
            {
                return Err(invalid("invalid or duplicate XISF uid"));
            }
        }
    }
    let mut blocks = 0;
    for node in &nodes {
        c.checkpoint()?;
        if node.name == "Reference" {
            let target = attr(node, "ref")?;
            if !identifiers.contains_key(target) {
                return Err(invalid(format!("unresolved XISF reference {target}")));
            }
            if node.attrs.contains_key("location") || !node.children.is_empty() {
                return Err(invalid("Reference cannot define a data block"));
            }
            continue;
        }
        if node.name == "Image" {
            c.images += 1;
        }
        if node.name == "Image" || node.name == "Thumbnail" {
            expected(node)?;
            attr(node, "location")?;
        }
        let has_descriptor = ["checksum", "compression", "subblocks"]
            .iter()
            .any(|key| node.attrs.contains_key(*key));
        if node.name == "Data" {
            let parent = node
                .parent
                .map(|i| &nodes[i])
                .ok_or_else(|| invalid("orphan Data element"))?;
            if parent.attrs.get("location").map(String::as_str) != Some("embedded")
                || node.attrs.contains_key("location")
            {
                return Err(invalid("Data requires a parent with embedded location"));
            }
        }
        let Some(location) = node.attrs.get("location") else {
            if has_descriptor && node.name != "Data" {
                return Err(invalid("data descriptor without block location"));
            }
            continue;
        };
        blocks += 1;
        c.structure()?;
        let (description, storage) = match BlockLocation::parse(location).map_err(structural)? {
            BlockLocation::Attachment { offset, size: len } => {
                if offset < envelope.xml_end() {
                    return Err(invalid("attachment overlaps header"));
                }
                c.extent(offset, len)?;
                (node, Storage::Attached(offset, len))
            }
            BlockLocation::Inline { encoding } => {
                (node, Storage::Inline(inline(c, node, &encoding)?))
            }
            BlockLocation::Embedded => {
                if has_descriptor {
                    return Err(invalid(
                        "embedded checksum/compression descriptors belong on Data",
                    ));
                }
                let children: Vec<_> = node
                    .children
                    .iter()
                    .map(|i| &nodes[*i])
                    .filter(|n| n.name == "Data")
                    .collect();
                if children.len() != 1 {
                    return Err(invalid("embedded block needs exactly one Data element"));
                }
                let d = children[0];
                (d, Storage::Inline(inline(c, d, attr(d, "encoding")?)?))
            }
            BlockLocation::Other(_) => {
                return Err(unsupported(format!("XISF block location {location}")));
            }
        };
        let comp = compression(c, description, storage.len())?;
        let length = comp.as_ref().map_or(storage.len(), |p| p.decoded);
        if let Some(expected) = expected(node)? {
            if expected != length {
                return Err(invalid(format!(
                    "{} expected {expected} bytes, layout declares {length}",
                    node.name
                )));
            }
        }
        let byte_order = node
            .image
            .as_ref()
            .and_then(|image| image.byte_order.as_deref())
            .or_else(|| node.attrs.get("byteOrder").map(String::as_str));
        if let Some(order) = byte_order {
            if order != "little" && order != "big" {
                return Err(invalid("invalid byte order"));
            }
        }
        c.decoded(length)?;
        checksum(c, description, &storage)
            .map_err(|e| e.context(format!("{} block {blocks} ({location})", node.name)))?;
        if c.options.level == ValidationLevel::Structural {
            if let Some(comp) = &comp {
                c.undecoded(&comp.codec)?;
            }
        }
        if c.options.level == ValidationLevel::Full {
            if let Some(comp) = comp {
                decode(c, &storage, &comp)
                    .map_err(|e| e.context(format!("{} block {blocks} ({location})", node.name)))?;
            } else {
                storage.stream(c, |_| {})?;
            }
        }
    }
    if blocks == 0 && c.images > 0 {
        return Err(invalid("image container has no data blocks"));
    }
    Ok(())
}
fn decode(c: &mut Context<'_>, storage: &Storage, compression: &Compression) -> Result<()> {
    if !["zlib", "lz4", "lz4hc", "zstd"].contains(&compression.codec.as_str()) {
        return Err(unsupported(format!(
            "compression codec {}",
            compression.codec
        )));
    }
    let mut offset = 0;
    for &(input, output) in &compression.parts {
        c.checkpoint()?;
        match compression.codec.as_str() {
            "zlib" => streaming::zlib(c, storage, offset, input, output)?,
            "zstd" => streaming::zstd(c, storage, offset, input, output)?,
            _ => {
                // Raw LZ4 needs complete input and output blocks. Inline input is
                // borrowed; attached input and decoded output own reservations.
                let bytes = storage.part(c, offset, input)?;
                let size = usize::try_from(output)
                    .map_err(|_| limit("LZ4 output exceeds address space"))?;
                let mut decoded = c.memory.buffer(size)?;
                let count = lz4_flex::block::decompress_into(&bytes, &mut decoded)
                    .map_err(|e| integrity(format!("LZ4 decode: {e}")))?;
                if count as u64 != output {
                    return Err(integrity("LZ4 decoded size mismatch"));
                }
            }
        }
        // Shuffling is a byte permutation; validation need not allocate or
        // reconstruct numeric pixels. Descriptor validity was checked earlier.
        if compression.shuffle == Some(0) {
            return Err(invalid("zero shuffle item size"));
        }
        offset = add(offset, input)?;
    }
    Ok(())
}
