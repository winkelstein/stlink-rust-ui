//! Probe list kept in sync with USB hot-plug (`nusb::watch_devices`): a
//! worker per probe, `device-event` to the frontend on every change.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_lite::StreamExt;
use stlink::ProbeDescriptor;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use crate::device::Actor;
use crate::dto::{self, DeviceEvent, ProbeSummary};
use crate::error::{BackendError, Result};

pub const DEVICE_EVENT: &str = "device-event";

struct Entry {
    actor: Arc<Actor>,
    info: ProbeSummary,
}

#[derive(Default)]
pub struct Probes(Mutex<HashMap<String, Entry>>);

impl Probes {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn list(&self) -> Vec<ProbeSummary> {
        let mut v: Vec<_> = self.lock().values().map(|e| e.info.clone()).collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    pub fn actor(&self, id: &str) -> Result<Arc<Actor>> {
        self.lock()
            .get(id)
            .map(|e| e.actor.clone())
            .ok_or_else(BackendError::disconnected)
    }

    /// Diff the current USB probes against the known ones: start workers for
    /// new probes (without opening them), drop the workers of removed ones.
    fn sync(&self, app: &AppHandle) {
        let present = match stlink::list_probes() {
            Ok(p) => p,
            Err(e) => return warn!("cannot list probes: {e}"),
        };
        let present: HashMap<String, ProbeDescriptor> = present
            .into_iter()
            .map(|d| (dto::probe_id(&d), d))
            .collect();

        let gone: Vec<String> = self
            .lock()
            .keys()
            .filter(|id| !present.contains_key(*id))
            .cloned()
            .collect();
        for id in gone {
            self.lock().remove(&id);
            info!("probe {id} disconnected");
            emit(app, DeviceEvent::Disconnected { probe_id: id });
        }

        for (id, desc) in present {
            if self.lock().contains_key(&id) {
                continue;
            }
            let info = ProbeSummary::from(&desc);
            match Actor::spawn(desc) {
                Ok(actor) => {
                    info!("probe {id} found: {}", info.descriptor.serial);
                    self.lock().insert(
                        id,
                        Entry {
                            actor: Arc::new(actor),
                            info: info.clone(),
                        },
                    );
                    emit(app, DeviceEvent::Connected { probe: info });
                }
                Err(e) => warn!("cannot start worker for probe {id}: {e}"),
            }
        }
    }
}

fn emit(app: &AppHandle, ev: DeviceEvent) {
    if let Err(e) = app.emit(DEVICE_EVENT, ev) {
        warn!("cannot emit {DEVICE_EVENT}: {e}");
    }
}

/// Initial scan, then rescan on every hot-plug event. Runs on its own thread
/// (enumeration and opening probes block).
pub fn start(app: AppHandle, probes: Arc<Probes>) {
    let spawned = std::thread::Builder::new()
        .name("hotplug".into())
        .spawn(move || {
            // Subscribe before the first scan so nothing plugged in between is lost.
            let watch = nusb::watch_devices();
            probes.sync(&app);
            let mut watch = match watch {
                Ok(w) => w,
                Err(e) => return warn!("hot-plug unavailable: {e}"),
            };
            while futures_lite::future::block_on(watch.next()).is_some() {
                probes.sync(&app);
            }
        });
    if let Err(e) = spawned {
        warn!("cannot start hot-plug thread: {e}");
    }
}
