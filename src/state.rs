use fs2::FileExt;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{Result, model::State};

pub(crate) struct BuildLock {
    file: fs::File,
}

impl BuildLock {
    pub(crate) fn acquire(root: &Path) -> Result<Self> {
        let directory = root.join(".need");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let path = directory.join("lock");
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| format!("could not open {}: {e}", path.display()))?;
        file.lock_exclusive()
            .map_err(|e| format!("could not acquire {}: {e}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub(crate) fn state_path(r: &Path) -> PathBuf {
    r.join(".need/state.json")
}
pub(crate) fn load_state(r: &Path) -> Result<State> {
    let p = state_path(r);
    if !p.exists() {
        return Ok(State::default());
    }
    serde_json::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub(crate) fn cleanup_recovery_files(r: &Path) -> Result<()> {
    let directory = r.join(".need");
    let Ok(entries) = fs::read_dir(&directory) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("state.json.") && name.ends_with(".tmp") {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    let capture_dir = directory.join("tmp");
    if capture_dir.is_dir() {
        for entry in fs::read_dir(capture_dir).map_err(|e| e.to_string())? {
            fs::remove_dir_all(entry.map_err(|e| e.to_string())?.path())
                .map_err(|e| e.to_string())?;
        }
    }
    cleanup_atomic_temporary_outputs(r)?;
    Ok(())
}

fn cleanup_atomic_temporary_outputs(root: &Path) -> Result<()> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_dir() {
            if name != ".need" && name != ".git" {
                cleanup_atomic_temporary_outputs(&path)?;
            }
        } else if name.starts_with(".need-tmp-") {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

static NEXT_STATE_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn save_state(r: &Path, s: &State) -> Result<()> {
    let p = state_path(r);
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    let t = p.with_file_name(format!(
        "state.json.{}-{}.tmp",
        std::process::id(),
        NEXT_STATE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&t)
        .map_err(|e| e.to_string())?;
    file.write_all(serde_json::to_string_pretty(s).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(t, p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("need-state-{suffix}"));
        fs::create_dir_all(root.join(".need/tmp/abandoned")).unwrap();
        fs::create_dir_all(root.join("nested")).unwrap();
        root
    }

    #[test]
    fn state_save_replaces_atomically_and_cleans_abandoned_files() {
        let root = temp_root();
        let state = State::default();
        save_state(&root, &state).unwrap();
        fs::write(root.join(".need/state.json.abandoned.tmp"), b"{").unwrap();
        fs::write(root.join(".need/tmp/abandoned/stdout"), b"partial").unwrap();
        fs::write(root.join("nested/.need-tmp-stale-output"), b"partial").unwrap();

        cleanup_recovery_files(&root).unwrap();
        assert!(!root.join(".need/state.json.abandoned.tmp").exists());
        assert!(!root.join(".need/tmp/abandoned").exists());
        assert!(!root.join("nested/.need-tmp-stale-output").exists());
        assert_eq!(
            serde_json::to_string(&load_state(&root).unwrap()).unwrap(),
            serde_json::to_string(&state).unwrap()
        );
        assert!(fs::read_dir(root.join(".need")).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hash_cache_state_round_trips() {
        let root = temp_root();
        let mut state = State::default();
        state.hashes.insert(
            "/project/input".into(),
            crate::model::HashRecord {
                size: 4,
                mtime_ns: 12,
                blake3: "hash".into(),
            },
        );
        save_state(&root, &state).unwrap();
        assert_eq!(load_state(&root).unwrap().hashes, state.hashes);
        fs::remove_dir_all(root).unwrap();
    }
}
