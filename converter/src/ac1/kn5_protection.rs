//! CSP KN5 protection (encVersion 1-3): spots the envelope so protected
//! models are refused. This file decrypts nothing and holds no keys.
//!
//! Envelope layout as documented by Kn5Decrypt (GPL-3.0) by x4fab / ilja615.
//! Envelope: the original KN5 body, then a sidecar of encrypted textures and
//! vertex masks, then a 38-byte tail: 30-byte magic + sidecar_offset (u32) +
//! enc_version (u32).

use anyhow::{bail, Result};

const MAGIC: &[u8] = b"__AC_SHADERS_PATCH_KN5ENC_v1__";
const TAIL_LEN: usize = MAGIC.len() + 8;

// ⚠ Protected mods are refused (LEGAL-REVIEW.md). An envelope with nothing
// encrypted in it (small helper kn5s) is not a protection and still opens.
const PROTECTED: &str = "this model is protected with CSP encryption, so it can't be converted. Ask the mod's author for an unprotected version";

/// The plain body when the envelope holds nothing encrypted; `None` when it does.
fn plain_body(data: &[u8]) -> Result<Option<Vec<u8>>> {
    let (off, _) = detect(data).ok_or_else(|| anyhow::anyhow!("no CSP v1 envelope"))?;
    Ok(sidecar_is_empty(data, off as usize, data.len() - TAIL_LEN).then(|| data[..off as usize].to_vec()))
}

/// Can this file be converted? `Ok(())` when nothing in it is protected,
/// `Err` (the message the user sees) when it is.
pub fn status(path: &std::path::Path) -> Result<()> {
    match plain_body(&std::fs::read(path)?)? {
        Some(_) => Ok(()),
        None => bail!("{PROTECTED}"),
    }
}

/// Returns `Some((sidecar_offset, enc_version))` when the file has a CSP v1 envelope.
pub fn detect(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < TAIL_LEN + 32 {
        return None;
    }
    let tail_start = data.len() - TAIL_LEN;
    if &data[tail_start..tail_start + MAGIC.len()] != MAGIC {
        return None;
    }
    let off = u32::from_le_bytes(data[data.len() - 8..data.len() - 4].try_into().unwrap());
    let ver = u32::from_le_bytes(data[data.len() - 4..].try_into().unwrap());
    Some((off, ver))
}

/// kn5 bytes: memory-mapped when plain, an owned copy of the body when an
/// unprotected envelope had to be cut off.
pub enum Kn5Bytes {
    Map(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl std::ops::Deref for Kn5Bytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Kn5Bytes::Map(m) => m,
            Kn5Bytes::Owned(v) => v,
        }
    }
}

/// Open a kn5 for reading. A file with an empty CSP envelope opens as its
/// plain body; a protected one is refused with the message above.
pub fn open(path: &std::path::Path) -> Result<Kn5Bytes> {
    let f = std::fs::File::open(path)?;
    // SAFETY: read-only map of a file nobody writes while we convert
    let mapped = unsafe { memmap2::Mmap::map(&f)? };
    if detect(&mapped).is_none() {
        return Ok(Kn5Bytes::Map(mapped));
    }
    Ok(Kn5Bytes::Owned(plain_body(&mapped)?.ok_or_else(|| anyhow::anyhow!("{PROTECTED}"))?))
}

// ── sidecar ──────────────────────────────────────────────────────────

/// Does the sidecar between `start` and `end` hold no protected entries
/// (texture `xxx.name` or vertex-mask entries)? Only names and lengths are read.
fn sidecar_is_empty(data: &[u8], start: usize, end: usize) -> bool {
    let mut pos = start;
    while pos < end {
        if pos + 4 > end { break; }
        let name_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + name_len > end { break; }
        let name = std::str::from_utf8(&data[pos..pos + name_len]).unwrap_or("");
        pos += name_len;
        if pos + 4 > end { break; }
        let data_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + data_len > end { break; }
        let n = name.len();
        if n > 6 && name.as_bytes()[3] == b'.' && name.as_bytes()[n - 2] == b'.' {
            return false;
        }
        pos += data_len;
    }
    true
}
