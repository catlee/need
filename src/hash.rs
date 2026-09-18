use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::Result;

pub(crate) fn walk(p: &Path) -> Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    for e in fs::read_dir(p).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let q = e.path();
        if e.file_type().map_err(|e| e.to_string())?.is_dir() {
            v.extend(walk(&q)?)
        } else {
            v.push(q)
        }
    }
    v.sort();
    Ok(v)
}
pub(crate) fn hash_file(p: &Path) -> Result<String> {
    let mut f = fs::File::open(p).map_err(|e| e.to_string())?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = f.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
pub(crate) fn hash_symlink(p: &Path) -> Result<String> {
    let target = fs::read_link(p).map_err(|e| e.to_string())?;
    Ok(hash_text(&format!("symlink:{}", target.to_string_lossy())))
}
pub(crate) fn hash_text(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex().to_string()
}
