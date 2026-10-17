//! Tauri backend of the GUI: commands and events behind `src/backend/TauriBackend.ts`.

mod device;
mod dto;
mod error;
mod files;
mod hotplug;
mod logging;
mod settings;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tracing::{error, info, warn};

use crate::device::CancelFlags;
use crate::dto::{
    BinFile, ConnectOptionsDto, DeviceSession, EraseResult, FlashPlan, ProbeSummary, ProgressDto,
    ReadJob, ReadResult, SaveTarget, WriteJob, WriteResult,
};
use crate::error::{BackendError, Result};
use crate::files::FileWatchers;
use crate::hotplug::Probes;
use crate::logging::{LogEntry, Logs};
use crate::settings::{AppSettingsStore, QuickWriteStore};

struct AppState {
    probes: Arc<Probes>,
    cancel: CancelFlags,
    watchers: FileWatchers,
    quick: QuickWriteStore,
    settings: AppSettingsStore,
    logs: Logs,
    downloads: PathBuf,
    home: Option<PathBuf>,
}

impl AppState {
    /// `~/Downloads` style path for the UI.
    /// The path as the user knows it: `~/Downloads` on macOS and Linux, the
    /// full native path (`C:\Users\…\Downloads`) on Windows.
    fn display_dir(&self, dir: &Path) -> String {
        if cfg!(windows) {
            return dir.display().to_string();
        }
        match self.home.as_deref().and_then(|h| dir.strip_prefix(h).ok()) {
            Some(rest) if rest.as_os_str().is_empty() => "~".into(),
            Some(rest) => format!("~/{}", rest.display()),
            None => dir.display().to_string(),
        }
    }

    fn save_target(&self, path: &Path) -> SaveTarget {
        SaveTarget {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            dir: path
                .parent()
                .map_or_else(String::new, |d| self.display_dir(d)),
            path: Some(path.to_string_lossy().into_owned()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickWriteEntry {
    uid: String,
    settings: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitData {
    downloads_dir: String,
    downloads_display: String,
    /// Native path separator, for joining file names in the UI.
    path_sep: String,
    quick_write: Vec<QuickWriteEntry>,
    settings: Value,
    log_path: Option<String>,
}

#[tauri::command]
fn init(state: State<'_, AppState>) -> InitData {
    InitData {
        downloads_dir: state.downloads.to_string_lossy().into_owned(),
        downloads_display: state.display_dir(&state.downloads),
        path_sep: std::path::MAIN_SEPARATOR.to_string(),
        quick_write: state
            .quick
            .all()
            .into_iter()
            .map(|(uid, settings)| QuickWriteEntry { uid, settings })
            .collect(),
        settings: state.settings.get(),
        log_path: state
            .logs
            .file
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    }
}

#[tauri::command]
fn list_devices(state: State<'_, AppState>) -> Vec<ProbeSummary> {
    state.probes.list()
}

/// The user's «Подключиться» (also with other options from an error banner).
#[tauri::command]
async fn connect(
    state: State<'_, AppState>,
    probe_id: String,
    opts: ConnectOptionsDto,
) -> Result<DeviceSession> {
    let actor = state.probes.actor(&probe_id)?;
    actor.call(move |w| w.connect_target(opts.into())).await
}

/// «Отключить»: close the probe so other programs can use it.
#[tauri::command]
async fn disconnect(state: State<'_, AppState>, probe_id: String) -> Result<()> {
    let actor = state.probes.actor(&probe_id)?;
    actor
        .call(|w| {
            w.disconnect();
            Ok(())
        })
        .await
}

#[tauri::command]
async fn plan_write(
    state: State<'_, AppState>,
    probe_id: String,
    addr: u32,
    len: u64,
) -> Result<FlashPlan> {
    let actor = state.probes.actor(&probe_id)?;
    actor.call(move |w| w.plan_write(addr, len)).await
}

#[tauri::command]
async fn write_flash(
    state: State<'_, AppState>,
    probe_id: String,
    job: WriteJob,
    op_id: String,
    on_progress: Channel<ProgressDto>,
) -> Result<WriteResult> {
    let actor = state.probes.actor(&probe_id)?;
    let cancel = state.cancel.register(&op_id);
    let r = actor
        .call(move |w| {
            w.write(job, &cancel, |p| {
                let _ = on_progress.send(p);
            })
        })
        .await;
    state.cancel.remove(&op_id);
    r
}

#[tauri::command]
async fn read_flash(
    state: State<'_, AppState>,
    probe_id: String,
    job: ReadJob,
    op_id: String,
    on_progress: Channel<ProgressDto>,
) -> Result<ReadResult> {
    let actor = state.probes.actor(&probe_id)?;
    let cancel = state.cancel.register(&op_id);
    let r = actor
        .call(move |w| {
            w.read(job, &cancel, |p| {
                let _ = on_progress.send(p);
            })
        })
        .await;
    state.cancel.remove(&op_id);
    r
}

#[tauri::command]
async fn erase_all(
    state: State<'_, AppState>,
    probe_id: String,
    op_id: String,
    on_progress: Channel<ProgressDto>,
) -> Result<EraseResult> {
    let actor = state.probes.actor(&probe_id)?;
    let cancel = state.cancel.register(&op_id);
    let r = actor
        .call(move |w| {
            w.erase_all(&cancel, |p| {
                let _ = on_progress.send(p);
            })
        })
        .await;
    state.cancel.remove(&op_id);
    r
}

#[tauri::command]
fn cancel_op(state: State<'_, AppState>, op_id: String) {
    state.cancel.cancel(&op_id);
}

#[tauri::command]
async fn reset_and_run(state: State<'_, AppState>, probe_id: String) -> Result<()> {
    let actor = state.probes.actor(&probe_id)?;
    actor.call(|w| w.reset_and_run()).await
}

#[tauri::command]
async fn pick_bin_file(app: AppHandle) -> Result<Option<BinFile>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Прошивка", &["bin"])
        .pick_file(move |p| {
            let _ = tx.send(p);
        });
    let picked = rx.await.ok().flatten();
    match picked.map(|p| p.into_path()) {
        None => Ok(None),
        Some(Ok(path)) => files::bin_file(path).map(Some),
        Some(Err(e)) => Err(BackendError::other(e.to_string())),
    }
}

#[tauri::command]
fn open_bin_file(path: String) -> Result<BinFile> {
    files::bin_file(path)
}

#[tauri::command]
async fn pick_save_path(
    app: AppHandle,
    state: State<'_, AppState>,
    default_name: String,
) -> Result<Option<SaveTarget>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Прошивка", &["bin"])
        .set_directory(&state.downloads)
        .set_file_name(&default_name)
        .save_file(move |p| {
            let _ = tx.send(p);
        });
    let picked = rx.await.ok().flatten();
    match picked.map(|p| p.into_path()) {
        None => Ok(None),
        Some(Ok(path)) => Ok(Some(state.save_target(&path))),
        Some(Err(e)) => Err(BackendError::other(e.to_string())),
    }
}

#[tauri::command]
fn reveal_in_finder(app: AppHandle, path: String) -> Result<()> {
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| BackendError::other(format!("cannot reveal {path}: {e}")))
}

/// Driver download pages linked from the «driver missing» screen; nothing
/// else can be opened from the UI.
const DRIVER_PAGES: [&str; 2] = [
    "https://www.st.com/en/development-tools/stsw-link009.html",
    "https://zadig.akeo.ie/",
];

#[tauri::command]
fn open_driver_page(app: AppHandle, url: String) -> Result<()> {
    if !DRIVER_PAGES.contains(&url.as_str()) {
        return Err(BackendError::other("unknown link"));
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| BackendError::other(format!("cannot open {url}: {e}")))
}

#[tauri::command]
fn watch_file(
    state: State<'_, AppState>,
    path: String,
    on_change: Channel<BinFile>,
) -> Result<u64> {
    state.watchers.watch(PathBuf::from(path), move |f| {
        let _ = on_change.send(f);
    })
}

#[tauri::command]
fn unwatch_file(state: State<'_, AppState>, id: u64) {
    state.watchers.unwatch(id);
}

#[tauri::command]
fn save_quick_write(state: State<'_, AppState>, uid: String, settings: Value) -> Result<()> {
    state.quick.save(uid, settings)
}

/// App settings; «Подробный журнал USB» takes effect right away.
#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Value) -> Result<()> {
    state.settings.save(settings)?;
    state.logs.set_verbose(state.settings.verbose_usb());
    Ok(())
}

#[tauri::command]
fn log_entries(state: State<'_, AppState>) -> Vec<LogEntry> {
    state.logs.entries()
}

#[tauri::command]
fn log_clear(state: State<'_, AppState>) {
    state.logs.clear();
}

/// Save the journal text (formatted by the frontend) through a save dialog;
/// returns the path, `None` if cancelled.
#[tauri::command]
async fn log_save(
    app: AppHandle,
    state: State<'_, AppState>,
    default_name: String,
    text: String,
) -> Result<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Log", &["log", "txt"])
        .set_directory(&state.downloads)
        .set_file_name(&default_name)
        .save_file(move |p| {
            let _ = tx.send(p);
        });
    let Some(picked) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|e| BackendError::other(e.to_string()))?;
    std::fs::write(&path, text)
        .map_err(|e| BackendError::other(format!("cannot write {}: {e}", path.display())))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn setup(app: &mut tauri::App) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let paths = app.path();
    let config_dir = paths
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("stlink-rust-ui"));
    let settings = AppSettingsStore::load(config_dir);
    let logs = logging::init(paths.app_log_dir().ok().as_deref(), settings.verbose_usb());
    logs.start_events(app.handle().clone());
    info!(
        "stlink-rust-ui {} started, log file: {:?}",
        env!("CARGO_PKG_VERSION"),
        logs.file
    );
    let home = paths.home_dir().ok();
    let downloads = paths
        .download_dir()
        .or_else(|_| paths.home_dir())
        .unwrap_or_else(|_| std::env::temp_dir());
    let data_dir = paths.app_data_dir().unwrap_or_else(|e| {
        warn!("no app data dir ({e}), settings are kept in the temp dir");
        std::env::temp_dir().join("stlink-rust-ui")
    });

    let probes = Arc::new(Probes::default());
    app.manage(AppState {
        probes: probes.clone(),
        cancel: CancelFlags::default(),
        watchers: FileWatchers::default(),
        quick: QuickWriteStore::load(data_dir),
        settings,
        logs,
        downloads,
        home,
    });
    hotplug::start(app.handle().clone(), probes);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            init,
            list_devices,
            connect,
            disconnect,
            plan_write,
            write_flash,
            read_flash,
            erase_all,
            cancel_op,
            reset_and_run,
            pick_bin_file,
            open_bin_file,
            pick_save_path,
            reveal_in_finder,
            open_driver_page,
            watch_file,
            unwatch_file,
            save_quick_write,
            save_settings,
            log_entries,
            log_clear,
            log_save,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        error!("error while running tauri application: {e}");
        eprintln!("error while running tauri application: {e}");
        std::process::exit(1);
    }
}
