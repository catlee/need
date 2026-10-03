use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::Result;

pub(crate) fn walk(p: &Path, follow_symlinks: bool, exclusions: &[String]) -> Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    let mut ancestors = HashSet::new();
    walk_into(p, p, follow_symlinks, exclusions, &mut ancestors, &mut v)?;
    v.sort();
    Ok(v)
}

fn walk_into(
    p: &Path,
    root: &Path,
    follow_symlinks: bool,
    exclusions: &[String],
    ancestors: &mut HashSet<PathBuf>,
    paths: &mut Vec<PathBuf>,
) -> Result<()> {
    let identity = fs::canonicalize(p).map_err(|e| e.to_string())?;
    if !ancestors.insert(identity.clone()) {
        return Ok(());
    }
    for entry in fs::read_dir(p).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if exclusions
            .iter()
            .any(|excluded| path.strip_prefix(root).unwrap().starts_with(excluded))
        {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_dir() {
            walk_into(&path, root, follow_symlinks, exclusions, ancestors, paths)?;
        } else {
            paths.push(path.clone());
            if follow_symlinks && file_type.is_symlink() {
                let Ok(target) = fs::metadata(&path) else {
                    continue;
                };
                if target.is_dir() {
                    walk_into(&path, root, true, exclusions, ancestors, paths)?;
                }
            }
        }
    }
    ancestors.remove(&identity);
    Ok(())
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
