//! Logging: the in-app journal (ring buffer + `log://entry` events), a log
//! file for bug reports and stderr. One filter for the journal and the file,
//! switched on the fly: `stlink` TRACE (USB packet dumps) only with the
//! «Подробный журнал USB» setting. `nusb` is off: it duplicates our errors.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber, warn};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry, reload};

pub const LOG_EVENT: &str = "log://entry";
/// Journal size, lines.
const CAPACITY: usize = 5000;
/// New entries are sent to the frontend in batches at most this often.
const FLUSH_EVERY: Duration = Duration::from_millis(100);

const NORMAL: &str = "info,stlink=debug,stlink_rust_ui_lib=debug,tauri=warn,nusb=off";
const VERBOSE: &str = "info,stlink=trace,stlink_rust_ui_lib=debug,tauri=warn,nusb=off";
const STDERR_FILTER: &str = "info,stlink=info,nusb=off";

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub seq: u64,
    /// ms since the Unix epoch.
    pub ts: u64,
    pub level: &'static str,
    pub target: String,
    pub message: String,
}

#[derive(Default)]
struct Journal {
    seq: u64,
    entries: VecDeque<LogEntry>,
    /// Not yet sent to the frontend.
    pending: Vec<LogEntry>,
}

/// Handle to the journal and the filter; cheap to clone.
#[derive(Clone)]
pub struct Logs {
    journal: Arc<Mutex<Journal>>,
    filter: reload::Handle<EnvFilter, Registry>,
    pub file: Option<PathBuf>,
}

impl Logs {
    fn lock(&self) -> std::sync::MutexGuard<'_, Journal> {
        self.journal.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.lock().entries.iter().cloned().collect()
    }

    pub fn clear(&self) {
        let mut j = self.lock();
        j.entries.clear();
        j.pending.clear();
    }

    /// TRACE for `stlink` on/off, effective immediately.
    pub fn set_verbose(&self, on: bool) {
        let spec = if on { VERBOSE } else { NORMAL };
        if let Err(e) = self.filter.reload(EnvFilter::new(spec)) {
            warn!("cannot change the log filter: {e}");
        }
    }

    /// Send new entries as `log://entry` (an array) every `FLUSH_EVERY`.
    pub fn start_events(&self, app: AppHandle) {
        let logs = self.clone();
        let spawned = std::thread::Builder::new()
            .name("log events".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(FLUSH_EVERY);
                    let batch = std::mem::take(&mut logs.lock().pending);
                    if !batch.is_empty() {
                        // Not logged on failure: that would feed the journal again.
                        let _ = app.emit(LOG_EVENT, batch);
                    }
                }
            });
        if let Err(e) = spawned {
            warn!("cannot start the log event thread: {e}");
        }
    }
}

/// `message` plus the other fields as ` key=value`.
#[derive(Default)]
struct MessageVisitor(String, String);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.1, " {}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            let _ = write!(self.1, " {}={value}", field.name());
        }
    }
}

struct JournalLayer(Arc<Mutex<Journal>>);

impl<S: Subscriber> Layer<S> for JournalLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut v = MessageVisitor::default();
        event.record(&mut v);
        let meta = event.metadata();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let mut j = self.0.lock().unwrap_or_else(|e| e.into_inner());
        j.seq += 1;
        let entry = LogEntry {
            seq: j.seq,
            ts,
            level: meta.level().as_str(),
            target: meta.target().to_owned(),
            message: v.0 + &v.1,
        };
        if j.entries.len() == CAPACITY {
            j.entries.pop_front();
        }
        j.entries.push_back(entry.clone());
        if j.pending.len() == CAPACITY {
            j.pending.remove(0);
        }
        j.pending.push(entry);
    }
}

/// Install the subscriber. The log file lives in `dir` (previous run kept as
/// `stlink-ui.old.log`). `RUST_LOG` overrides the stderr filter.
pub fn init(dir: Option<&Path>, verbose: bool) -> Logs {
    let path = dir.map(|d| d.join("stlink-ui.log"));
    let file = dir.zip(path.as_ref()).and_then(|(dir, path)| {
        std::fs::create_dir_all(dir)
            .and_then(|()| {
                if path.exists() {
                    std::fs::rename(path, dir.join("stlink-ui.old.log"))?;
                }
                std::fs::File::create(path)
            })
            .map_err(|e| eprintln!("cannot create log file {}: {e}", path.display()))
            .ok()
    });

    let (filter, handle) =
        reload::Layer::new(EnvFilter::new(if verbose { VERBOSE } else { NORMAL }));
    let journal = Arc::new(Mutex::new(Journal::default()));
    let stderr_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(STDERR_FILTER));
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(stderr_filter);
    let has_file = file.is_some();
    let file_layer = file.map(|f| {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_thread_names(true)
            .with_writer(Mutex::new(f))
    });
    let installed = tracing_subscriber::registry()
        .with(filter)
        .with(JournalLayer(journal.clone()))
        .with(file_layer)
        .with(stderr)
        .try_init();
    Logs {
        journal,
        filter: handle,
        file: path.filter(|_| installed.is_ok() && has_file),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_keeps_last_entries() {
        let journal = Arc::new(Mutex::new(Journal::default()));
        let sub = tracing_subscriber::registry().with(JournalLayer(journal.clone()));
        tracing::subscriber::with_default(sub, || {
            for i in 0..CAPACITY + 10 {
                tracing::info!(n = i, "line");
            }
        });
        let j = journal.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(j.entries.len(), CAPACITY);
        assert_eq!(j.entries.front().map(|e| e.seq), Some(11));
        assert_eq!(
            j.entries.back().map(|e| e.message.as_str()),
            Some(format!("line n={}", CAPACITY + 9).as_str())
        );
        assert_eq!(j.entries.back().map(|e| e.level), Some("INFO"));
    }
}
