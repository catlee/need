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

// Length prefixes keep raw path and link bytes unambiguous.
pub(crate) fn hash_directory(root: &Path) -> Result<String> {
    fn entry_error(path: &Path, error: impl std::fmt::Display) -> String {
        format!(
            "could not fingerprint directory entry {}: {error}\nhelp: check that the entry exists and is readable",
            path.display()
        )
    }
    fn bytes(hasher: &mut blake3::Hasher, value: &[u8]) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    let metadata = fs::symlink_metadata(root).map_err(|error| entry_error(root, error))?;
    if !metadata.is_dir() {
        return Err(format!(
            "directory output {} is not a directory\nhelp: preserve the staging directory",
            root.display()
        ));
    }
    let mut paths = vec![root.to_path_buf()];
    let mut index = 0;
    while index < paths.len() {
        let path = &paths[index];
        let metadata = fs::symlink_metadata(path).map_err(|error| entry_error(path, error))?;
        if metadata.is_dir() {
            let entries = fs::read_dir(path)
                .map_err(|error| entry_error(path, error))?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()
                .map_err(|error| entry_error(path, error))?;
            paths.extend(entries);
        }
        index += 1;
    }
    paths.sort_by(|a, b| {
        a.as_os_str()
            .as_encoded_bytes()
            .cmp(b.as_os_str().as_encoded_bytes())
    });
    let mut hasher = blake3::Hasher::new();
    for path in paths {
        let metadata = fs::symlink_metadata(&path).map_err(|error| entry_error(&path, error))?;
        bytes(
            &mut hasher,
            path.strip_prefix(root)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes(),
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hasher.update(&(metadata.permissions().mode() & 0o7777).to_le_bytes());
        }
        let kind = metadata.file_type();
        if kind.is_symlink() {
            hasher.update(b"link");
            bytes(
                &mut hasher,
                fs::read_link(&path)
                    .map_err(|error| entry_error(&path, error))?
                    .as_os_str()
                    .as_encoded_bytes(),
            );
        } else if kind.is_file() {
            hasher.update(b"file");
            bytes(
                &mut hasher,
                hash_file(&path)
                    .map_err(|error| entry_error(&path, error))?
                    .as_bytes(),
            );
        } else if kind.is_dir() {
            hasher.update(b"directory");
        } else {
            return Err(format!(
                "unsupported entry in directory output: {}\nhelp: produce only regular files, directories, and symlinks",
                path.display()
            ));
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}
