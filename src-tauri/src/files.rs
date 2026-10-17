//! Firmware files: metadata (size, mtime, CRC), reading, change watching.
//! A file's id is its absolute path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use notify::{RecursiveMode, Watcher};
use tracing::{debug, warn};

use crate::dto::BinFile;
use crate::error::{BackendError, ErrorKind, Result};

/// Larger files cannot be firmware for any supported chip.
const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;

fn is_bin(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
}

fn io_error(path: &Path, e: std::io::Error) -> BackendError {
    BackendError::other(format!("cannot read {}: {e}", path.display()))
}

/// Read a `.bin` with the size limit.
pub fn read_bin(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    if !is_bin(path) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        return Err(BackendError::new(
            ErrorKind::Unsupported,
            format!("«{name}»: поддерживаются только файлы .bin"),
        ));
    }
    let len = std::fs::metadata(path).map_err(|e| io_error(path, e))?.len();
    if len > MAX_FILE_SIZE {
        return Err(BackendError::other(format!(
            "{}: file too large ({len} bytes)",
            path.display()
        )));
    }
    std::fs::read(path).map_err(|e| io_error(path, e))
}

/// Metadata of a `.bin` (reads it for the CRC).
pub fn bin_file(path: impl AsRef<Path>) -> Result<BinFile> {
    let path = path.as_ref();
    let data = read_bin(path)?;
    let modified_at = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    let path_str = path.to_string_lossy().into_owned();
    Ok(BinFile {
        id: path_str.clone(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: Some(path_str),
        size: data.len() as u64,
        modified_at,
        crc32: stlink::flash::crc32(&data),
    })
}

/// Active file watchers by id.
#[derive(Default)]
pub struct FileWatchers {
    next: AtomicU64,
    watchers: Mutex<HashMap<u64, notify::RecommendedWatcher>>,
}

impl FileWatchers {
    /// Watch `path` for changes; `on_change` gets the new metadata. The parent
    /// directory is watched since build tools often replace the file (rename).
    pub fn watch(
        &self,
        path: PathBuf,
        on_change: impl Fn(BinFile) + Send + 'static,
    ) -> Result<u64> {
        let dir = path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| BackendError::other("invalid file path"))?;
        let name = path.file_name().map(|n| n.to_os_string());
        let mut last: Option<BinFile> = None;
        let watched = path.clone();
        let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
            let ev = match ev {
                Ok(ev) => ev,
                Err(e) => return warn!("file watcher: {e}"),
            };
            if !ev.paths.iter().any(|p| p.file_name() == name.as_deref()) {
                return;
            }
            // Mid-write the file may be missing or short; the next event follows.
            let Ok(meta) = bin_file(&watched) else {
                return;
            };
            if last.as_ref() != Some(&meta) {
                debug!(
                    "{} changed: {} bytes, CRC 0x{:08X}",
                    meta.name, meta.size, meta.crc32
                );
                last = Some(meta.clone());
                on_change(meta);
            }
        })
        .map_err(|e| BackendError::other(format!("cannot watch {}: {e}", path.display())))?;
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|e| BackendError::other(format!("cannot watch {}: {e}", dir.display())))?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock().insert(id, watcher);
        Ok(id)
    }

    pub fn unwatch(&self, id: u64) {
        self.lock().remove(&id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, notify::RecommendedWatcher>> {
        self.watchers.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, data: &[u8]) -> std::io::Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!("stlink-ui-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let p = dir.join(name);
        std::fs::write(&p, data)?;
        Ok(p)
    }

    #[test]
    fn bin_file_metadata() -> std::io::Result<()> {
        let p = temp("fw.bin", b"123456789")?;
        let f = bin_file(&p);
        assert!(matches!(&f, Ok(f) if f.size == 9 && f.crc32 == 0xCBF4_3926 && f.name == "fw.bin"));
        Ok(())
    }

    #[test]
    fn rejects_non_bin() -> std::io::Result<()> {
        let p = temp("fw.hex", b":00000001FF")?;
        assert!(matches!(read_bin(&p), Err(e) if e.kind == ErrorKind::Unsupported));
        Ok(())
    }
}
