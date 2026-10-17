//! Persistent frontend settings, opaque JSON owned by the frontend:
//! - quick write per chip UID: `quick-write.json` in the app data dir (file
//!   metadata refreshed on load);
//! - app settings: `settings.json` in the app config dir.

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Map, Value};
use tracing::warn;

use crate::error::{BackendError, Result};
use crate::files;

pub struct QuickWriteStore {
    path: PathBuf,
    data: Mutex<Map<String, Value>>,
}

impl QuickWriteStore {
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("quick-write.json");
        let data = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Map<String, Value>>(&bytes).unwrap_or_else(|e| {
                warn!("ignoring {}: {e}", path.display());
                Map::new()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
            Err(e) => {
                warn!("cannot read {}: {e}", path.display());
                Map::new()
            }
        };
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Map<String, Value>> {
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// All entries, with the stored file metadata replaced by the current one
    /// (the file may have been rebuilt while the app was closed). Settings of
    /// older versions are migrated (`onConnect` is gone).
    pub fn all(&self) -> Vec<(String, Value)> {
        let entries: Vec<(String, Value)> = self
            .lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        entries
            .into_iter()
            .map(|(uid, mut s)| {
                if let Some(obj) = s.as_object_mut() {
                    obj.remove("onConnect");
                }
                refresh_file(&mut s);
                (uid, s)
            })
            .collect()
    }

    pub fn save(&self, uid: String, settings: Value) -> Result<()> {
        let json = {
            let mut data = self.lock();
            data.insert(uid, settings);
            serde_json::to_vec_pretty(&*data).map_err(|e| BackendError::other(e.to_string()))?
        };
        write_atomic(&self.path, &json)
            .map_err(|e| BackendError::other(format!("cannot save {}: {e}", self.path.display())))
    }
}

/// App settings (`settings.json`): language, theme, flashing and probe options.
pub struct AppSettingsStore {
    path: PathBuf,
    data: Mutex<Value>,
}

impl AppSettingsStore {
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("settings.json");
        let data = std::fs::read(&path)
            .ok()
            .and_then(|b| {
                serde_json::from_slice::<Value>(&b)
                    .map_err(|e| warn!("ignoring {}: {e}", path.display()))
                    .ok()
            })
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Value> {
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self) -> Value {
        self.lock().clone()
    }

    /// «Verbose USB log».
    pub fn verbose_usb(&self) -> bool {
        self.lock()
            .get("verboseUsb")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn save(&self, settings: Value) -> Result<()> {
        let json =
            serde_json::to_vec_pretty(&settings).map_err(|e| BackendError::other(e.to_string()))?;
        *self.lock() = settings;
        write_atomic(&self.path, &json)
            .map_err(|e| BackendError::other(format!("cannot save {}: {e}", self.path.display())))
    }
}

fn refresh_file(settings: &mut Value) {
    let Some(file) = settings.get_mut("file") else {
        return;
    };
    let Some(path) = file.get("path").and_then(Value::as_str) else {
        return;
    };
    // Missing file: keep the stored metadata; writing reports the error.
    if let Ok(Ok(meta)) = files::bin_file(path).map(serde_json::to_value) {
        *file = meta;
    }
}

fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_reload() {
        let dir = std::env::temp_dir().join(format!("stlink-ui-settings-{}", std::process::id()));
        let store = QuickWriteStore::load(dir.clone());
        let s = serde_json::json!({ "addr": 0x0800_0000u32, "file": null, "onFileChange": true });
        assert!(store.save("UID1".into(), s.clone()).is_ok());
        let again = QuickWriteStore::load(dir);
        assert_eq!(again.all(), vec![("UID1".to_string(), s)]);
    }

    #[test]
    fn drops_on_connect_from_old_settings() {
        let dir = std::env::temp_dir().join(format!("stlink-ui-migrate-{}", std::process::id()));
        let store = QuickWriteStore::load(dir);
        let old = serde_json::json!({ "addr": 1, "onConnect": true, "onFileChange": false });
        assert!(store.save("U".into(), old).is_ok());
        let all = store.all();
        assert_eq!(
            all[0].1,
            serde_json::json!({ "addr": 1, "onFileChange": false })
        );
    }

    #[test]
    fn app_settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("stlink-ui-app-{}", std::process::id()));
        let store = AppSettingsStore::load(dir.clone());
        assert!(!store.verbose_usb());
        assert!(
            store
                .save(serde_json::json!({ "verboseUsb": true, "lang": "en" }))
                .is_ok()
        );
        let again = AppSettingsStore::load(dir);
        assert!(again.verbose_usb());
        assert_eq!(again.get()["lang"], "en");
    }
}
