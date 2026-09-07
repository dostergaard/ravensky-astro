use super::*;

fn header(cards: &[String]) -> Result<[u8; 2880]> {
    ensure!(cards.len() <= 36, "generated FITS header too large");
    let mut bytes = [b' '; 2880];
    for (i, card) in cards.iter().enumerate() {
        ensure!(card.len() <= 80, "generated FITS card too large");
        bytes[i * 80..i * 80 + card.len()].copy_from_slice(card.as_bytes());
    }
    Ok(bytes)
}

pub(super) fn generate(
    mut writer: LimitedWriter,
    recipe: &Recipe,
    frame: usize,
    cancel: &AtomicBool,
) -> Result<()> {
    let tile_rows = recipe.tile_rows.unwrap_or(recipe.height);
    let tiles = recipe.height.div_ceil(tile_rows);
    let heap = 5760 + u64::from(tiles) * 16;
    // Reserve header/descriptor space through the same quota writer as payloads.
    let mut remaining = heap;
    while remaining > 0 {
        checkpoint(cancel)?;
        let n = remaining.min(CHUNK as u64) as usize;
        writer.write_all(&[0; CHUNK][..n])?;
        remaining -= n as u64;
    }
    let mut pixels = Pixels::new(recipe, frame);
    for tile in 0..tiles {
        checkpoint(cancel)?;
        let start = writer.file.stream_position()?;
        let rows = tile_rows.min(recipe.height - tile * tile_rows);
        let count = u64::from(recipe.width) * u64::from(rows);
        let mut encoder =
            flate2::write::GzEncoder::new(&mut writer, flate2::Compression::default());
        if recipe.encoding == Encoding::FitsGzip2 {
            // Revisit deterministic state for the second byte plane. No tile buffer.
            pixel_chunk_stream(&mut encoder, &mut pixels.clone(), count, Some(0), cancel)?;
            pixel_chunk_stream(&mut encoder, &mut pixels, count, Some(1), cancel)?;
        } else {
            pixel_chunk_stream(&mut encoder, &mut pixels, count, None, cancel)?;
        }
        encoder.finish()?;
        let end = writer.file.stream_position()?;
        // Replace the already-accounted Q descriptor; never extend on backpatch.
        writer
            .file
            .seek(SeekFrom::Start(5760 + u64::from(tile) * 16))?;
        writer.file.write_all(&(end - start).to_be_bytes())?;
        writer.file.write_all(&(start - heap).to_be_bytes())?;
        writer.file.seek(SeekFrom::Start(end))?;
    }
    let size = writer.file.stream_position()?;
    writer.write_all(&[0; 2880][..((2880 - size % 2880) % 2880) as usize])?;
    writer.file.seek(SeekFrom::Start(0))?;
    writer.file.write_all(&header(&[
        "SIMPLE  =                    T".into(),
        "BITPIX  =                    8".into(),
        "NAXIS   =                    0".into(),
        "EXTEND  =                    T".into(),
        "END".into(),
    ])?)?;
    let codec = if recipe.encoding == Encoding::FitsGzip2 {
        "GZIP_2"
    } else {
        "GZIP_1"
    };
    writer.file.write_all(&header(&[
        "XTENSION= 'BINTABLE'".into(),
        "BITPIX  =                    8".into(),
        "NAXIS   =                    2".into(),
        "NAXIS1  =                   16".into(),
        format!("NAXIS2  = {tiles:>20}"),
        format!("PCOUNT  = {:>20}", size - heap),
        "GCOUNT  =                    1".into(),
        "TFIELDS =                    1".into(),
        "TTYPE1  = 'COMPRESSED_DATA'".into(),
        "TFORM1  = '1QB'".into(),
        "ZIMAGE  =                    T".into(),
        "ZBITPIX =                   16".into(),
        "ZNAXIS  =                    2".into(),
        format!("ZNAXIS1 = {:>20}", recipe.width),
        format!("ZNAXIS2 = {:>20}", recipe.height),
        format!("ZTILE1  = {:>20}", recipe.width),
        format!("ZTILE2  = {tile_rows:>20}"),
        format!("ZCMPTYPE= '{codec}'"),
        "BZERO   =                32768".into(),
        "BSCALE  =                    1".into(),
        "END".into(),
    ])?)?;
    writer.file.sync_all()?;
    Ok(())
}
