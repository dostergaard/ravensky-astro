//! Low-level format metadata writes. Callers supply truthful, eligible values;
//! semantic mapping and loss approval remain application responsibilities.

use anyhow::{bail, Context, Result};
use fitsio::FitsFile;
use std::{ffi::CString, path::Path};

/// Append exact 80-byte nonstructural FITS cards to one HDU, retaining caller
/// order and duplicates. Structural/scaling/compression/checksum cards are
/// rejected because changing them independently can corrupt pixel meaning.
/// The HDU checksum is recomputed after the append.
pub fn append_fits_raw_cards(path: &Path, hdu_index: usize, cards: &[String]) -> Result<()> {
    for card in cards {
        validate_raw_card(card)?;
    }
    crate::fits::backend::with_cfitsio(|| {
        let mut file = FitsFile::edit(path)?;
        let _ = file.hdu(hdu_index)?;
        let raw = unsafe { file.as_raw() };
        let mut status = 0;
        for card in cards {
            let value = CString::new(card.as_bytes()).context("FITS card contains NUL")?;
            // SAFETY: CString is live for each native call; handle is current
            // and exclusively owned under the coordinated backend gate.
            unsafe { fitsio::sys::fits_write_record(raw, value.as_ptr(), &mut status) };
            fitsio::errors::check_status(status)?;
        }
        // SAFETY: the current HDU remains selected and writable.
        unsafe { fitsio::sys::ffpcks(raw, &mut status) };
        fitsio::errors::check_status(status)?;
        Ok(())
    })
}

/// Append one short ASCII FITS HISTORY record and refresh the HDU checksum.
/// This primitive is suitable for later provenance values supplied by P2/P4.
pub fn append_fits_history(path: &Path, hdu_index: usize, text: &str) -> Result<()> {
    if text.is_empty() || text.len() > 72 || !text.bytes().all(|byte| (32..=126).contains(&byte)) {
        bail!("FITS HISTORY value must be 1..=72 printable ASCII bytes");
    }
    crate::fits::backend::with_cfitsio(|| {
        let mut file = FitsFile::edit(path)?;
        let _ = file.hdu(hdu_index)?;
        let raw = unsafe { file.as_raw() };
        let value = CString::new(text)?;
        let mut status = 0;
        // SAFETY: live CString and current writable HDU are enclosed by the gate.
        unsafe {
            fitsio::sys::fits_write_history(raw, value.as_ptr(), &mut status);
            if status == 0 {
                fitsio::sys::ffpcks(raw, &mut status);
            }
        }
        fitsio::errors::check_status(status)?;
        Ok(())
    })
}

fn validate_raw_card(card: &str) -> Result<()> {
    if card.len() != 80 || !card.is_ascii() || card.bytes().any(|byte| !(32..=126).contains(&byte))
    {
        bail!("FITS raw card must contain exactly 80 printable ASCII bytes");
    }
    let keyword = card[..8].trim();
    if keyword.is_empty()
        || keyword == "END"
        || matches!(
            keyword,
            "SIMPLE"
                | "XTENSION"
                | "BITPIX"
                | "PCOUNT"
                | "GCOUNT"
                | "TFIELDS"
                | "EXTEND"
                | "BZERO"
                | "BSCALE"
                | "ZIMAGE"
                | "ZBITPIX"
                | "ZCMPTYPE"
                | "CHECKSUM"
                | "DATASUM"
        )
        || keyword.starts_with("NAXIS")
        || keyword.starts_with("ZNAXIS")
        || keyword.starts_with("ZTILE")
    {
        bail!("FITS structural or checksum card {keyword} cannot be appended");
    }
    Ok(())
}
