use super::*;
use base64::Engine;
use quick_xml::{events::Event, name::ResolveResult, reader::NsReader};
use sha2::Digest;
use std::{collections::BTreeMap, io::Cursor};

#[derive(Default)]
struct Node {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<usize>,
    parent: Option<usize>,
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
fn parse(c: &mut Context<'_>, xml: &[u8]) -> Result<Vec<Node>> {
    let mut reader = NsReader::from_reader(xml);
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack = Vec::new();
    let mut roots = 0;
    loop {
        c.checkpoint()?;
        let (ns, event) = reader
            .read_resolved_event()
            .map_err(|e| invalid(format!("invalid XISF XML: {e}")))?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                c.structure()?;
                // Namespace-less headers are accepted for compatibility with
                // common producers and existing fixtures. Other namespaces are
                // not interpreted as XISF layout elements.
                let accepted = match ns {
                    ResolveResult::Unbound => true,
                    ResolveResult::Bound(n) => n.as_ref() == b"http://www.pixinsight.com/xisf",
                    ResolveResult::Unknown(_) => return Err(invalid("unbound XML namespace")),
                };
                let name = std::str::from_utf8(e.local_name().as_ref())
                    .map_err(|_| invalid("invalid element name"))?
                    .to_string();
                if !accepted {
                    return Err(unsupported(format!("XML namespace on {name}")));
                }
                if stack.is_empty() {
                    roots += 1;
                    if roots != 1 || name != "xisf" {
                        return Err(invalid("expected one xisf XML root"));
                    }
                }
                let mut node = Node {
                    name,
                    parent: stack.last().copied(),
                    ..Node::default()
                };
                for a in e.attributes() {
                    let a = a.map_err(|e| invalid(format!("invalid XML attribute: {e}")))?;
                    let key = std::str::from_utf8(a.key.as_ref())
                        .map_err(|_| invalid("invalid attribute name"))?
                        .to_string();
                    let value = a
                        .unescape_value()
                        .map_err(|e| invalid(format!("invalid attribute value: {e}")))?
                        .into_owned();
                    if node.attrs.insert(key, value).is_some() {
                        return Err(invalid("duplicate XML attribute"));
                    }
                }
                let idx = nodes.len();
                if let Some(&parent) = stack.last() {
                    let p: &mut Node = &mut nodes[parent];
                    p.children.push(idx);
                }
                nodes.push(node);
                c.memory(add(
                    mul(xml.len() as u64, 8)?,
                    mul(nodes.len() as u64, 256)?,
                )?)?;
                if matches!(event, Event::Start(_)) {
                    stack.push(idx);
                }
            }
            Event::End(_) => {
                if stack.pop().is_none() {
                    return Err(invalid("unexpected XML closing element"));
                }
            }
            Event::Text(e) => {
                let text = e
                    .unescape()
                    .map_err(|e| invalid(format!("invalid XML text: {e}")))?;
                if let Some(&idx) = stack.last() {
                    nodes[idx].text.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err(invalid("text outside XML root"));
                }
            }
            Event::CData(e) => {
                let text =
                    std::str::from_utf8(e.as_ref()).map_err(|_| invalid("invalid XML UTF-8"))?;
                if let Some(&idx) = stack.last() {
                    nodes[idx].text.push_str(text);
                } else {
                    return Err(invalid("CDATA outside XML root"));
                }
            }
            Event::DocType(_) => {
                return Err(unsupported("XML DTD/entity declarations are not supported"))
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if roots != 1 || !stack.is_empty() {
        return Err(invalid("incomplete XML document"));
    }
    if attr(&nodes[0], "version")? != "1.0" {
        return Err(unsupported("XISF version other than 1.0"));
    }
    Ok(nodes)
}
fn sample_size(value: &str) -> Result<u64> {
    match value {
        "UInt8" => Ok(1),
        "UInt16" => Ok(2),
        "UInt32" | "Float32" => Ok(4),
        "UInt64" | "Float64" | "Complex32" => Ok(8),
        "Complex64" => Ok(16),
        _ => Err(unsupported(format!("sample format {value}"))),
    }
}
fn expected(n: &Node) -> Result<Option<u64>> {
    if n.name == "Image" || n.name == "Thumbnail" {
        let geometry = attr(n, "geometry")?;
        let dims: Vec<_> = geometry.split(':').collect();
        if dims.len() < 2 {
            return Err(invalid(
                "image geometry needs spatial dimensions and channels",
            ));
        }
        let mut count = 1;
        for d in dims {
            let d = natural(d)?;
            if d == 0 {
                return Err(invalid("zero image dimension"));
            }
            count = mul(count, d)?;
        }
        return Ok(Some(mul(count, sample_size(attr(n, "sampleFormat")?)?)?));
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
    Inline(Vec<u8>),
}
impl Storage {
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
    fn part(&self, c: &mut Context<'_>, offset: u64, size: u64) -> Result<Vec<u8>> {
        match self {
            Self::Attached(o, _) => c.bytes(add(*o, offset)?, size),
            Self::Inline(b) => {
                let start =
                    usize::try_from(offset).map_err(|_| limit("offset exceeds address space"))?;
                let end = usize::try_from(add(offset, size)?)
                    .map_err(|_| limit("offset exceeds address space"))?;
                Ok(b.get(start..end)
                    .ok_or_else(|| invalid("subblock exceeds inline payload"))?
                    .to_vec())
            }
        }
    }
}
fn inline(c: &Context<'_>, node: &Node, encoding: &str) -> Result<Vec<u8>> {
    if !node.children.is_empty() {
        return Err(invalid("inline Data has child elements"));
    }
    c.memory(mul(node.text.len() as u64, 2)?)?;
    let text: Vec<u8> = node
        .text
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    match encoding {
        "base64" => base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|_| invalid("invalid Base64 block")),
        "hex" => hex(&text),
        _ => Err(unsupported(format!("inline encoding {encoding}"))),
    }
}
fn hex(bytes: &[u8]) -> Result<Vec<u8>> {
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid("odd hexadecimal length"));
    }
    bytes
        .chunks_exact(2)
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
    let mut prefix = [0; 16];
    c.read(0, &mut prefix)?;
    if prefix[12..] != [0; 4] {
        return Err(invalid("XISF reserved prefix bytes must be zero"));
    }
    let size = u32::from_le_bytes(
        prefix[8..12]
            .try_into()
            .map_err(|_| invalid("XISF header length"))?,
    ) as u64;
    c.header(size)?;
    c.memory(mul(size, 8)?)?;
    let xml = c.bytes(16, size)?;
    let nodes = parse(c, &xml)?;
    drop(xml);
    let reserved = add(mul(size, 8)?, mul(nodes.len() as u64, 256)?)?;
    c.memory(reserved)?;
    c.reserved = reserved;
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
        let (description, storage) = if let Some(rest) = location.strip_prefix("attachment:") {
            let (offset, len) = rest
                .split_once(':')
                .ok_or_else(|| invalid("invalid attachment location"))?;
            let (offset, len) = (natural(offset)?, natural(len)?);
            if offset < add(16, size)? {
                return Err(invalid("attachment overlaps header"));
            }
            c.extent(offset, len)?;
            (node, Storage::Attached(offset, len))
        } else if let Some(encoding) = location.strip_prefix("inline:") {
            (node, Storage::Inline(inline(c, node, encoding)?))
        } else if location == "embedded" {
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
        } else {
            return Err(unsupported(format!("XISF block location {location}")));
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
        if let Some(order) = node.attrs.get("byteOrder") {
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
    c.reserved = 0;
    Ok(())
}
fn decode(c: &mut Context<'_>, s: &Storage, p: &Compression) -> Result<()> {
    if !["zlib", "lz4", "lz4hc", "zstd"].contains(&p.codec.as_str()) {
        return Err(unsupported(format!("compression codec {}", p.codec)));
    }
    let mut offset = 0;
    for &(input, output) in &p.parts {
        c.checkpoint()?;
        // Each codec subblock is indivisible. Reject oversized declarations
        // before allocation; stored inline bytes remain live as well.
        let inline = if matches!(s, Storage::Inline(_)) {
            s.len()
        } else {
            0
        };
        c.memory(add(add(input, mul(output, 2)?)?, add(inline, 1 << 20)?)?)?;
        let bytes = s.part(c, offset, input)?;
        let out_len =
            usize::try_from(output).map_err(|_| limit("decoded block exceeds address space"))?;
        let mut decoded = Vec::new();
        decoded
            .try_reserve_exact(out_len)
            .map_err(|_| limit("decoded buffer allocation failed"))?;
        match p.codec.as_str() {
            "lz4" | "lz4hc" => {
                decoded.resize(out_len, 0);
                let n = lz4_flex::block::decompress_into(&bytes, &mut decoded)
                    .map_err(|e| integrity(format!("LZ4 decode: {e}")))?;
                if n != out_len {
                    return Err(integrity("LZ4 decoded size mismatch"));
                }
            }
            "zlib" => {
                let mut decoder = flate2::bufread::ZlibDecoder::new(Cursor::new(&bytes));
                drain(c, &mut decoder, &mut decoded, output)?;
                if decoder.total_in() != input {
                    return Err(integrity("trailing or unused zlib bytes"));
                }
            }
            "zstd" => {
                let mut decoder = zstd::stream::read::Decoder::with_buffer(Cursor::new(&bytes))
                    .map_err(|e| integrity(format!("Zstandard: {e}")))?;
                // Restrict the native decoder window to the remaining budget.
                let remaining = c
                    .options
                    .limits
                    .working
                    .saturating_sub(c.reserved)
                    .saturating_sub(c.diagnostic_bytes)
                    .saturating_sub(input)
                    .saturating_sub(output * 2)
                    .saturating_sub(inline)
                    .saturating_sub(1 << 20);
                if remaining < 1024 {
                    return Err(limit("insufficient Zstandard window budget"));
                }
                decoder
                    .window_log_max(remaining.ilog2().min(30))
                    .map_err(|e| limit(format!("Zstandard window limit: {e}")))?;
                drain(c, &mut decoder, &mut decoded, output)?;
            }
            _ => return Err(unsupported("compression codec")),
        }
        // Unshuffling is a byte permutation, not a numeric conversion. Check
        // its parameters without allocating a second full output image.
        if let Some(item) = p.shuffle {
            if item == 0 {
                return Err(invalid("zero shuffle item size"));
            }
        }
        offset = add(offset, input)?;
    }
    Ok(())
}
fn drain(
    c: &Context<'_>,
    reader: &mut impl Read,
    output: &mut Vec<u8>,
    expected: u64,
) -> Result<()> {
    let mut chunk = [0; 65536];
    loop {
        c.checkpoint()?;
        let remaining = expected.saturating_sub(output.len() as u64);
        let n = reader
            .read(&mut chunk[..(remaining.saturating_add(1)).min(65536) as usize])
            .map_err(|e| integrity(format!("payload decode: {e}")))?;
        if n == 0 {
            break;
        }
        if output.len() as u64 + n as u64 > expected {
            return Err(integrity("decoded payload exceeds declared size"));
        }
        output.extend_from_slice(&chunk[..n]);
    }
    if output.len() as u64 != expected {
        return Err(integrity("decoded payload shorter than declared size"));
    }
    Ok(())
}
