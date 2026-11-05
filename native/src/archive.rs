//! Zip and tar (plain or gzip) behind one face, for the AC1 mod archives
//! dropped on the Convert page - ported from ACECM's install.py (zipfile and
//! `tarfile.open(path, "r:*")`).

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

fn is_zip(path: &Path) -> bool {
    path.to_string_lossy().to_lowercase().ends_with(".zip")
}

fn tar_reader(path: &Path) -> std::io::Result<Box<dyn Read>> {
    let mut f = BufReader::new(File::open(path)?);
    let mut magic = [0u8; 2];
    let n = f.read(&mut magic)?;
    let f = BufReader::new(File::open(path)?);
    let _ = n;
    if magic == [0x1f, 0x8b] {
        Ok(Box::new(flate2::read::GzDecoder::new(f)))
    } else {
        Ok(Box::new(f))
    }
}

fn tar_name(e: &tar::Entry<'_, Box<dyn Read>>) -> String {
    e.path().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default()
}

fn tar_is_file(e: &tar::Entry<'_, Box<dyn Read>>) -> bool {
    matches!(e.header().entry_type(), tar::EntryType::Regular | tar::EntryType::Continuous)
}

/// A member's reader that yields at most its DECLARED size and fails if the
/// stream holds more. ⚠ The zip crate bounds the compressed input, not the
/// output: a member declaring 1 KB could inflate to 100 GB and fill the
/// disk past the size check on the declared sizes (convert.rs; Python's
/// zipfile stops at the declared size).
struct Capped<'a> {
    inner: &'a mut dyn Read,
    left: u64,
}

impl Read for Capped<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.left == 0 {
            let mut probe = [0u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(std::io::Error::other("archive member is larger than it declares")),
            };
        }
        let want = buf.len().min(self.left as usize);
        let n = self.inner.read(&mut buf[..want])?;
        self.left -= n as u64;
        Ok(n)
    }
}

/// Call `f(name, size, reader)` for each file, in archive order; `f`
/// returns false to stop early.
pub fn each_file(path: &Path, mut f: impl FnMut(&str, u64, &mut dyn Read) -> Result<bool, String>) -> Result<(), String> {
    if is_zip(path) {
        let mut z = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        for i in 0..z.len() {
            let mut m = z.by_index(i).map_err(|e| e.to_string())?;
            if m.is_dir() {
                continue;
            }
            let (name, size) = (m.name().to_string(), m.size());
            let mut capped = Capped { inner: &mut m, left: size };
            if !f(&name, size, &mut capped)? {
                break;
            }
        }
        return Ok(());
    }
    let mut t = tar::Archive::new(tar_reader(path).map_err(|e| e.to_string())?);
    for e in t.entries().map_err(|e| e.to_string())? {
        let mut e = e.map_err(|e| e.to_string())?;
        if !tar_is_file(&e) {
            continue;
        }
        let (name, size) = (tar_name(&e), e.header().size().unwrap_or(0));
        if !f(&name, size, &mut e)? {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_members_read_whole_and_capped() {
        use std::io::Write;
        let p = std::env::temp_dir().join(format!("lsp_cap_{}.zip", std::process::id()));
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
            z.start_file("a/b.txt", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(&vec![7u8; 100_000]).unwrap();
            z.finish().unwrap();
        }
        let mut n = 0;
        each_file(&p, |name, size, r| {
            let mut v = Vec::new();
            r.read_to_end(&mut v).map_err(|e| e.to_string())?;
            assert_eq!((name, size, v.len()), ("a/b.txt", 100_000, 100_000));
            n += 1;
            Ok(true)
        }).unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_file(p);
    }
}
