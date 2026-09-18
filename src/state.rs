use fs2::FileExt;
use std::{
    fs,
    path::{Path, PathBuf},
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
pub(crate) fn save_state(r: &Path, s: &State) -> Result<()> {
    let p = state_path(r);
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    let t = p.with_extension("tmp");
    fs::write(&t, serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(t, p).map_err(|e| e.to_string())
}
