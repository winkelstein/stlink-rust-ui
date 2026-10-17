//! Log file for bug reports: `stlink` at `trace` (USB packet dumps), the app
//! at `debug`; the previous run is kept as `stlink-ui.old.log`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::{EnvFilter, Layer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// `nusb` is off: it duplicates our own error reports.
const FILE_FILTER: &str = "info,stlink=trace,stlink_rust_ui_lib=debug,nusb=off";
const STDERR_FILTER: &str = "info,stlink=info,nusb=off";

/// Install the subscriber; returns the log file path. `RUST_LOG` overrides
/// the console filter.
pub fn init(dir: &Path) -> Option<PathBuf> {
    let path = dir.join("stlink-ui.log");
    let file = std::fs::create_dir_all(dir)
        .and_then(|()| {
            if path.exists() {
                std::fs::rename(&path, dir.join("stlink-ui.old.log"))?;
            }
            std::fs::File::create(&path)
        })
        .map_err(|e| eprintln!("cannot create log file {}: {e}", path.display()))
        .ok();

    let stderr_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(STDERR_FILTER));
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(stderr_filter);
    let file_layer = file.map(|f| {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_thread_names(true)
            .with_writer(Mutex::new(f))
            .with_filter(EnvFilter::new(FILE_FILTER))
    });
    let has_file = file_layer.is_some();
    let installed = tracing_subscriber::registry()
        .with(stderr)
        .with(file_layer)
        .try_init();
    (installed.is_ok() && has_file).then_some(path)
}
