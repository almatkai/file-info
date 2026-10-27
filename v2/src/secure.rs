//! Selected ordinary-file handles, destination-owned staging and identity fencing.
use crate::{Result, MAX_FILE, MAX_PATH};
use std::{
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

#[cfg(unix)]
#[path = "secure_unix.rs"]
mod os;
#[cfg(windows)]
#[path = "secure_windows.rs"]
mod os;
pub use os::Stage;

pub struct Commit {
    pub detail: String,
    pub complete: bool,
}
pub struct Selected {
    pub path: PathBuf,
    pub file: File,
    pub parent: File,
    pub name: std::ffi::OsString,
    pub metadata: Metadata,
    pub _ancestors: Vec<File>,
}
pub fn validate_path(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.len() > MAX_PATH || value.contains('\0') {
        return Err("Invalid or over-limit selected path".into());
    }
    let p = Path::new(value);
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
    {
        return Err("Selection must contain absolute paths without traversal".into());
    }
    // components() normalizes '.', so check the spelling as well.
    #[cfg(unix)]
    if value.split('/').any(|c| c == "." || c == "..") {
        return Err("Selection path contains traversal".into());
    }
    #[cfg(windows)]
    if value.contains('/')
        || value.split('\\').any(|c| c == "." || c == "..")
        || value.starts_with("\\\\")
        || value[2..].contains(':')
    {
        return Err("Only ordinary local drive paths without ADS/traversal are supported".into());
    }
    Ok(p.to_path_buf())
}
impl Selected {
    pub fn open(value: &str, for_clear: bool) -> Result<Self> {
        let path = validate_path(value)?;
        let (parent, name, file, ancestors) = os::open(&path)?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !(metadata.is_file() || (!for_clear && metadata.is_dir())) {
            return Err("Only selected ordinary files (or read-only directory info) are supported; clear rejects directories, links and devices".into());
        }
        if for_clear {
            if metadata.len() > MAX_FILE {
                return Err("File exceeds 256 MiB clear limit".into());
            }
            os::clear_policy(&metadata)?;
        }
        Ok(Self {
            path,
            file,
            parent,
            name,
            metadata,
            _ancestors: ancestors,
        })
    }
    pub fn recheck(&self) -> Result<()> {
        let (parent, _, file, _) = os::open(&self.path)?;
        let current = file.metadata().map_err(|e| e.to_string())?;
        if !os::same_file(&self.file, &file)?
            || !os::same(&self.metadata, &current)
            || !os::same(
                &self.metadata,
                &self.file.metadata().map_err(|e| e.to_string())?,
            )
            || !os::same_file(&self.parent, &parent)?
        {
            return Err(
                "Selected file identity/content changed; frozen operation was not retried".into(),
            );
        }
        Ok(())
    }
    pub fn read(&mut self) -> Result<Vec<u8>> {
        self.recheck()?;
        if self.metadata.len() > MAX_FILE {
            return Err("File exceeds 256 MiB validation limit".into());
        }
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::with_capacity(self.metadata.len() as usize);
        (&mut self.file)
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        self.recheck()?;
        if bytes.len() as u64 != self.metadata.len() {
            return Err("Selected file changed while reading".into());
        }
        Ok(bytes)
    }
    pub fn stage(&self, extension: &str, bytes: &[u8]) -> Result<Stage> {
        if !self.metadata.is_file() {
            return Err("Directories cannot be staged for clearing".into());
        }
        self.recheck()?;
        let mut stage = Stage::new(self, extension)?;
        stage.file.write_all(bytes).map_err(|e| e.to_string())?;
        stage.file.sync_all().map_err(|e| e.to_string())?;
        Ok(stage)
    }
    pub fn commit(&self, stage: &mut Stage) -> Result<Commit> {
        if !self.metadata.is_file() {
            return Err("Directories cannot be cleared".into());
        }
        stage.verify()?;
        os::commit(self, stage)
    }
    pub fn frozen_identity(&self) -> Result<serde_json::Value> {
        os::identity(&self.file, &self.metadata)
    }
    pub fn mode(&self) -> String {
        os::mode(&self.metadata)
    }
}
/// Inherit exactly one already-open ordinary file or directory to a utility.
/// All user-controlled path data stays outside shell source.
pub fn anchor(command: &mut tokio::process::Command, file: &File, leaf: Option<&str>) -> String {
    os::anchor(command, file, leaf)
}
