use anyhow::{Context, Result};
use astro_io::xisf::{read_metadata_records, read_xisf_xml_header, XisfMetadataRecords};
use astro_metadata::xisf_parser::extract_metadata_from_path;
use quick_xml::{events::Event, Reader, Writer};
use std::fs::File;
use std::io::Seek;
use std::path::Path;

#[path = "shared/metadata_dump.rs"]
mod metadata_dump;

fn main() {
    metadata_dump::run_metadata_dump_with_postscript(
        "XISF",
        "<xisf_file_path>",
        "Raw XISF FITS Keywords",
        extract_metadata_from_path,
        dump_xisf_diagnostics,
    );
}

fn dump_xisf_diagnostics(path: &Path) -> Result<()> {
    let mut file = File::open(path).context("Failed to open XISF file for raw diagnostics")?;
    let records =
        read_metadata_records(&mut file).context("Failed to inspect XISF raw format records")?;
    file.rewind()
        .context("Failed to rewind XISF file before reading XML header")?;
    let xml = read_xisf_xml_header(&mut file).context("Failed to read XISF XML header")?;

    print_raw_format(&records);
    println!("\nXISF XML Document\n=================");
    println!("{}", pretty_print_xml(&xml)?);
    Ok(())
}

fn print_raw_format(records: &XisfMetadataRecords) {
    println!("\nXISF Raw Format\n---------------");
    print_raw_value("Version", records.version());
    print_raw_value("Block Alignment", records.block_alignment());

    for (index, image) in records.images().iter().enumerate() {
        println!("Image {}", index + 1);
        print_raw_value("  Id", image.id());
        print_raw_value("  Geometry", image.geometry());
        print_raw_value("  Sample Format", image.sample_format());
        print_raw_value("  Bits Per Sample", image.bits_per_sample());
        print_raw_value("  Color Space", image.color_space());
        print_raw_value("  Compression", image.compression());
        print_raw_value("  Compression Parameters", image.compression_parameters());
        print_raw_value("  Checksum Type", image.checksum_type());
        print_raw_value("  Checksum", image.checksum());
    }
}

fn print_raw_value(label: &str, value: Option<&str>) {
    if let Some(value) = value {
        println!("{label}: {value}");
    }
}

/// Re-emits generic XML events with indentation. It does not interpret XISF
/// vocabulary, so unknown writer content remains part of the diagnostic view.
fn pretty_print_xml(xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    let mut buffer = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buffer)
            .context("Failed to parse XISF XML for display")?
        {
            Event::Eof => break,
            event => writer
                .write_event(event)
                .context("Failed to format XISF XML for display")?,
        }
        buffer.clear();
    }

    let formatted = String::from_utf8(writer.into_inner())
        .context("XML formatter produced non-UTF-8 output")?;
    Ok(formatted
        .strip_prefix('\n')
        .unwrap_or(&formatted)
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::pretty_print_xml;

    #[test]
    fn pretty_print_xml_retains_unknown_content_and_document_order() {
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<xisf xmlns:writer="urn:writer" version="1.0">"#,
            r#"<!-- writer comment -->"#,
            r#"<writer:Unknown writer:flag="yes">writer text</writer:Unknown>"#,
            r#"<Image geometry="2:1:3" sampleFormat="UInt16"/>"#,
            r#"</xisf>"#,
        );

        let formatted = pretty_print_xml(xml).expect("representative XML should format");

        assert!(formatted.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(formatted.contains("xmlns:writer=\"urn:writer\""));
        assert!(formatted.contains("<!-- writer comment -->"));
        assert!(
            formatted.contains("<writer:Unknown writer:flag=\"yes\">writer text</writer:Unknown>")
        );
        assert!(formatted.contains("<Image geometry=\"2:1:3\" sampleFormat=\"UInt16\"/>"));

        let comment = formatted.find("<!-- writer comment -->").unwrap();
        let unknown = formatted.find("<writer:Unknown").unwrap();
        let image = formatted.find("<Image geometry").unwrap();
        assert!(comment < unknown && unknown < image);
    }
}
