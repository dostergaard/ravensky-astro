//! FITS file loading and header extraction helpers.

pub mod backend;

use anyhow::{bail, Context, Result};
use fitsio::errors::check_status;
use fitsio::sys::{
    ffthdu, fits_get_hdrspace, fits_read_keyn, fits_read_record, FLEN_CARD, FLEN_COMMENT,
    FLEN_KEYWORD, FLEN_VALUE,
};
use fitsio::FitsFile;
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;

/// A single FITS header card.
///
/// This is the canonical, lossless(ish) representation used by the metadata layer.
/// The exact raw card text is preserved when it is available from an on-disk FITS file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FitsHeaderCard {
    /// Zero-based HDU index containing this card.
    pub hdu_index: usize,
    /// One-based position of the card within the HDU header.
    pub card_index: usize,
    /// Keyword name as reported by CFITSIO.
    pub keyword: String,
    /// Parsed value, if the card has one.
    pub value: Option<String>,
    /// Parsed comment text, if the card has one.
    pub comment: Option<String>,
    /// Exact raw 80-character FITS card when available.
    pub raw_card: Option<String>,
}

/// Read a FITS file and return its pixel data, width, and height.
pub fn load_fits(path: &Path) -> Result<(Vec<f32>, usize, usize)> {
    backend::with_cfitsio(|| {
        // Open the FITS file.
        let mut file = FitsFile::open(path)?;
        // Access the primary HDU (header-data unit).
        let hdu = file.primary_hdu()?;

        // Extract the image dimensions by borrowing hdu.info.
        let (width, height) = if let fitsio::hdu::HduInfo::ImageInfo { shape, .. } = &hdu.info {
            let h = shape[0];
            let w = shape[1];
            (w, h)
        } else {
            bail!("Primary HDU is not an image");
        };

        // Read the entire image into a Vec<f32>.
        let pixels: Vec<f32> = hdu.read_image(&mut file)?;

        Ok((pixels, width, height))
    })
}

/// Read all header cards from the primary HDU in a FITS file.
pub fn read_primary_header_cards_from_path(path: &Path) -> Result<Vec<FitsHeaderCard>> {
    backend::with_cfitsio(|| {
        let mut file = FitsFile::open(path)?;
        read_header_cards(&mut file, 0)
    })
}

/// Read all header cards from a specific HDU in an open FITS file.
///
/// Native calls participate in [`backend::with_cfitsio`]. On non-reentrant builds,
/// enclose the caller-owned handle's complete lifetime in that same protocol.
pub fn read_header_cards(
    fits_file: &mut FitsFile,
    hdu_index: usize,
) -> Result<Vec<FitsHeaderCard>> {
    backend::with_cfitsio(|| {
        let _ = fits_file
            .hdu(hdu_index)
            .with_context(|| format!("Failed to access HDU {}", hdu_index))?;

        let raw_fits = unsafe { fits_file.as_raw() };
        let mut num_keys = 0;
        let mut more_keys = 0;
        let mut status = 0;

        unsafe {
            fits_get_hdrspace(raw_fits, &mut num_keys, &mut more_keys, &mut status);
        }

        check_status(status)
            .with_context(|| format!("Failed to enumerate header cards for HDU {}", hdu_index))?;

        let mut cards = Vec::with_capacity(num_keys as usize);
        for card_index in 1..=num_keys {
            let raw_card = read_raw_card(raw_fits, card_index).with_context(|| {
                format!(
                    "Failed to read header card {} from HDU {}",
                    card_index, hdu_index
                )
            })?;
            let (keyword, value, comment) = read_card_fields(raw_fits, card_index, &raw_card);

            cards.push(FitsHeaderCard {
                hdu_index,
                card_index: card_index as usize,
                keyword,
                value,
                comment,
                raw_card: Some(raw_card),
            });
        }

        Ok(cards)
    })
}

/// Read all header cards from every HDU in an open FITS file.
///
/// On non-reentrant builds, enclose the caller-owned handle's complete lifetime
/// in [`backend::with_cfitsio`], including open and close.
pub fn read_all_header_cards(fits_file: &mut FitsFile) -> Result<Vec<FitsHeaderCard>> {
    backend::with_cfitsio(|| {
        let num_hdus = read_num_hdus(fits_file)?;
        let mut cards = Vec::new();

        for hdu_index in 0..num_hdus {
            cards.extend(read_header_cards(fits_file, hdu_index)?);
        }

        Ok(cards)
    })
}

/// Build a compatibility header map from lossless header cards.
///
/// If duplicate keywords are present, the last value wins.
pub fn header_cards_to_map(cards: &[FitsHeaderCard]) -> HashMap<String, String> {
    let mut headers = HashMap::new();

    for card in cards {
        if let Some(value) = &card.value {
            headers.insert(card.keyword.clone(), value.clone());
        }
    }

    headers
}

/// Normalize pixel values to a 0.0-1.0 range.
pub fn normalize_pixels(pixels: &[f32]) -> Vec<f32> {
    if pixels.is_empty() {
        return Vec::new();
    }

    // Find min and max values.
    let mut min_val = pixels[0];
    let mut max_val = pixels[0];

    for &pixel in pixels {
        min_val = min_val.min(pixel);
        max_val = max_val.max(pixel);
    }

    // Avoid division by zero.
    let range = max_val - min_val;
    if range == 0.0 {
        return vec![0.0; pixels.len()];
    }

    // Normalize each pixel.
    pixels.iter().map(|&p| (p - min_val) / range).collect()
}

fn read_num_hdus(fits_file: &mut FitsFile) -> Result<usize> {
    let raw_fits = unsafe { fits_file.as_raw() };
    let mut num_hdus = 0;
    let mut status = 0;

    unsafe {
        ffthdu(raw_fits, &mut num_hdus, &mut status);
    }

    check_status(status).context("Failed to count FITS HDUs")?;
    Ok(num_hdus as usize)
}

fn read_raw_card(raw_fits: *mut fitsio::sys::fitsfile, card_index: i32) -> Result<String> {
    let mut status = 0;
    let mut raw_card = vec![0 as c_char; FLEN_CARD as usize];

    unsafe {
        fits_read_record(raw_fits, card_index, raw_card.as_mut_ptr(), &mut status);
    }

    check_status(status)?;
    Ok(c_string_to_string(&raw_card))
}

fn read_card_fields(
    raw_fits: *mut fitsio::sys::fitsfile,
    card_index: i32,
    raw_card: &str,
) -> (String, Option<String>, Option<String>) {
    let mut status = 0;
    let mut keyword = vec![0 as c_char; FLEN_KEYWORD as usize];
    let mut value = vec![0 as c_char; FLEN_VALUE as usize];
    let mut comment = vec![0 as c_char; FLEN_COMMENT as usize];

    unsafe {
        fits_read_keyn(
            raw_fits,
            card_index,
            keyword.as_mut_ptr(),
            value.as_mut_ptr(),
            comment.as_mut_ptr(),
            &mut status,
        );
    }

    if status != 0 {
        return (parse_keyword_from_raw_card(raw_card), None, None);
    }

    let keyword = c_string_to_string(&keyword);
    let raw_value = c_string_to_string(&value);
    let comment = c_string_to_string(&comment);
    let cleaned_keyword = if keyword.is_empty() {
        parse_keyword_from_raw_card(raw_card)
    } else {
        keyword
    };

    let value = if raw_value.is_empty() {
        None
    } else {
        Some(clean_header_value(&raw_value))
    };

    let comment = if comment.is_empty() {
        None
    } else {
        Some(comment.trim().to_string())
    };

    (cleaned_keyword, value, comment)
}

fn clean_header_value(raw_value: &str) -> String {
    let trimmed = raw_value.trim();

    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        trimmed[1..trimmed.len() - 1]
            .replace("''", "'")
            .trim_end()
            .to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_keyword_from_raw_card(raw_card: &str) -> String {
    let trimmed = raw_card.trim_end();

    if trimmed.starts_with("HIERARCH") {
        if let Some((keyword, _)) = trimmed.split_once('=') {
            return keyword.trim().to_string();
        }

        return "HIERARCH".to_string();
    }

    raw_card
        .chars()
        .take(8)
        .collect::<String>()
        .trim()
        .to_string()
}

fn c_string_to_string(buffer: &[c_char]) -> String {
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fitsio::images::{ImageDescription, ImageType};
    use fitsio::sys::{fits_write_comment, fits_write_history, fits_write_record};
    use std::ffi::CString;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_normalize_pixels() {
        // Test with normal range.
        let pixels = vec![100.0, 200.0, 300.0, 400.0, 500.0];
        let normalized = normalize_pixels(&pixels);

        assert_eq!(normalized.len(), 5);
        assert_eq!(normalized[0], 0.0);
        assert_eq!(normalized[4], 1.0);
        assert!((normalized[2] - 0.5).abs() < 0.001);

        // Test with empty input.
        let empty: Vec<f32> = Vec::new();
        let result = normalize_pixels(&empty);
        assert_eq!(result.len(), 0);

        // Test with single value (avoid division by zero).
        let single = vec![42.0];
        let result = normalize_pixels(&single);
        assert_eq!(result, vec![0.0]);

        // Test with all same values.
        let same = vec![10.0, 10.0, 10.0];
        let result = normalize_pixels(&same);
        assert_eq!(result, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_extract_dimensions() {
        // This is a mock test since we can't easily create a FitsFile for testing.
        let shape = vec![1024, 768];
        let (width, height) = extract_dimensions_from_shape(&shape);

        assert_eq!(width, 768);
        assert_eq!(height, 1024);

        let shape = vec![2048, 1536];
        let (width, height) = extract_dimensions_from_shape(&shape);

        assert_eq!(width, 1536);
        assert_eq!(height, 2048);
    }

    #[test]
    fn test_read_header_cards_preserves_duplicates() -> Result<()> {
        crate::fits::backend::with_cfitsio(|| {
            let path = unique_temp_fits_path("header-cards");
            let mut file = FitsFile::create(&path).open()?;
            let hdu = file.primary_hdu()?;

            hdu.write_key(&mut file, "OBJECT", "M42".to_string())?;
            hdu.write_key(&mut file, "EXPTIME", 120.5f32)?;
            append_test_records(&mut file)?;

            let cards = read_header_cards(&mut file, 0)?;
            let duplicate_values: Vec<&str> = cards
                .iter()
                .filter(|card| card.keyword == "DUPKEY")
                .filter_map(|card| card.value.as_deref())
                .collect();

            assert_eq!(duplicate_values, vec!["one", "two"]);
            assert!(cards.iter().any(|card| {
                card.keyword == "COMMENT"
                    && card
                        .raw_card
                        .as_deref()
                        .is_some_and(|raw| raw.contains("first duplicate-preserving comment"))
            }));

            fs::remove_file(path)?;
            Ok(())
        })
    }

    #[test]
    fn test_read_all_header_cards_across_hdus() -> Result<()> {
        crate::fits::backend::with_cfitsio(|| {
            let path = unique_temp_fits_path("all-hdus");
            let description = ImageDescription {
                data_type: ImageType::Float,
                dimensions: &[2, 2],
            };
            let mut file = FitsFile::create(&path).open()?;
            let primary = file.primary_hdu()?;
            primary.write_key(&mut file, "OBJECT", "M31".to_string())?;

            let extension = file.create_image("SCI".to_string(), &description)?;
            extension.write_key(&mut file, "EXTKEY", 42i64)?;

            let cards = read_all_header_cards(&mut file)?;
            assert!(cards
                .iter()
                .any(|card| card.hdu_index == 0 && card.keyword == "OBJECT"));
            assert!(cards.iter().any(|card| card.hdu_index == 1
                && card.keyword == "EXTKEY"
                && card.value.as_deref() == Some("42")));

            fs::remove_file(path)?;
            Ok(())
        })
    }

    #[test]
    fn test_header_cards_to_map_uses_last_duplicate_value() {
        let cards = vec![
            FitsHeaderCard {
                hdu_index: 0,
                card_index: 1,
                keyword: "DUPKEY".to_string(),
                value: Some("one".to_string()),
                comment: None,
                raw_card: None,
            },
            FitsHeaderCard {
                hdu_index: 0,
                card_index: 2,
                keyword: "DUPKEY".to_string(),
                value: Some("two".to_string()),
                comment: None,
                raw_card: None,
            },
        ];

        let headers = header_cards_to_map(&cards);
        assert_eq!(headers.get("DUPKEY"), Some(&"two".to_string()));
    }

    // Helper function to test dimension extraction logic.
    fn extract_dimensions_from_shape(shape: &[usize]) -> (usize, usize) {
        let h = shape[0];
        let w = shape[1];
        (w, h)
    }

    fn append_test_records(file: &mut FitsFile) -> Result<()> {
        let mut status = 0;
        let raw_fits = unsafe { file.as_raw() };
        let comment = CString::new("first duplicate-preserving comment")?;
        let history = CString::new("history entry")?;
        let duplicate_one = CString::new("DUPKEY  = 'one'")?;
        let duplicate_two = CString::new("DUPKEY  = 'two'")?;

        unsafe {
            fits_write_comment(raw_fits, comment.as_ptr(), &mut status);
            fits_write_history(raw_fits, history.as_ptr(), &mut status);
            fits_write_record(raw_fits, duplicate_one.as_ptr(), &mut status);
            fits_write_record(raw_fits, duplicate_two.as_ptr(), &mut status);
        }

        check_status(status)?;
        Ok(())
    }

    fn unique_temp_fits_path(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "astro-io-{prefix}-{}-{timestamp}.fits",
            std::process::id()
        ))
    }
}
