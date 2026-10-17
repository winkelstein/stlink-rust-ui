//! One worker thread (actor) per probe. All `stlink` calls for a probe run on
//! its thread, one at a time; commands wait for the result through a oneshot.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use stlink::flash::{self, Progress, Stage};
use stlink::{ConnectOptions, Connection, ProbeDescriptor, StLink, TargetInfo};
use tracing::{debug, info, warn};

use crate::dto::{
    ConnectionDto, DeviceSession, EraseResult, FlashPlan, ProbeInfo, ProbeSummary,
    ProgressDto, ReadJob, ReadResult, StageMs, TargetInfoDto, WriteJob, WriteResult,
};
use crate::error::{BackendError, ErrorKind, Result};
use crate::files;

type Job = Box<dyn FnOnce(&mut Worker) + Send>;

/// Handle of a probe's worker thread. Dropping it stops the thread once the
/// current job is done; the thread then drops the `StLink` (leaves debug mode).
pub struct Actor {
    tx: mpsc::Sender<Job>,
}

impl Actor {
    /// Start the worker of a discovered probe. The probe is **not** opened:
    /// the app touches it only after the user's `connect`.
    pub fn spawn(desc: ProbeDescriptor) -> std::io::Result<Self> {
        let mut worker = Worker::new(desc);
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name(format!("probe {}", worker.info.id))
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    job(&mut worker);
                }
                debug!("probe {} worker stopped", worker.info.id);
            })?;
        Ok(Self { tx })
    }

    /// Run `f` on the worker thread and wait for its result.
    pub async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Worker) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Box::new(move |w| {
                let r = f(w);
                // The journal shows what the UI reports by kind only.
                if let Err(e) = &r
                    && e.kind != ErrorKind::Cancelled
                {
                    warn!("{:?}: {}", e.kind, e.message);
                }
                let _ = tx.send(r);
            }))
            .map_err(|_| BackendError::disconnected())?;
        rx.await.map_err(|_| BackendError::disconnected())?
    }
}

/// State of one probe, owned by its worker thread.
pub struct Worker {
    desc: ProbeDescriptor,
    info: ProbeInfo,
    link: Option<StLink>,
    /// Options of the user's `connect`; `None` while disconnected. Reused to
    /// reopen after an error dropped the link, never without a `connect`.
    opts: Option<ConnectOptions>,
    /// The identified target (its UID notices a swapped board).
    target: Option<TargetInfo>,
}

impl Worker {
    fn new(desc: ProbeDescriptor) -> Self {
        let summary = ProbeSummary::from(&desc);
        let info = ProbeInfo {
            id: summary.id,
            descriptor: summary.descriptor,
            version: None,
            target_voltage: None,
        };
        Self {
            desc,
            info,
            link: None,
            opts: None,
            target: None,
        }
    }

    /// (Re)open the probe and refresh version and voltage.
    fn open(&mut self) -> stlink::Result<&mut StLink> {
        self.link = None;
        let mut link = StLink::open(&self.desc)?;
        self.info.version = Some(link.version().into());
        self.info.target_voltage = match link.target_voltage() {
            Ok(v) => v,
            Err(e) => {
                warn!("cannot read target voltage: {e}");
                None
            }
        };
        Ok(self.link.insert(link))
    }

    /// Connect and resume a core halted by the connect sequence.
    fn connect(link: &mut StLink, opts: &ConnectOptions) -> stlink::Result<Connection> {
        let conn = link.connect(opts)?;
        link.release(&conn)?;
        Ok(conn)
    }

    /// The open, connected probe; reopens and reconnects (last options) after
    /// an error dropped it.
    fn link(&mut self) -> stlink::Result<&mut StLink> {
        if self.link.is_none() {
            let opts = self.opts.ok_or(stlink::Error::TargetNotConnected)?;
            let link = self.open()?;
            Self::connect(link, &opts)?;
        }
        self.link.as_mut().ok_or(stlink::Error::Disconnected)
    }

    /// Fresh `identify` before a destructive operation; the board must be the
    /// one the user saw (same UID).
    fn current_target(&mut self) -> Result<TargetInfo> {
        let known = self.target.as_ref().map(|t| t.uid);
        let target = self.link()?.identify()?;
        if known.is_some_and(|uid| uid != target.uid) {
            return Err(BackendError::other(
                "another chip is connected; identify the target again",
            ));
        }
        Ok(target)
    }

    /// Forget the connection after errors that leave its state unknown, so the
    /// next operation starts with a clean open + connect.
    fn after<T>(&mut self, r: Result<T>) -> Result<T> {
        if let Err(e) = &r
            && !matches!(
                e.kind,
                ErrorKind::Cancelled | ErrorKind::Unsupported | ErrorKind::FlashProtected
            )
        {
            debug!("dropping the probe connection after: {}", e.message);
            self.link = None;
        }
        r
    }

    /// The user's «Подключиться»: open, connect, identify. On failure the
    /// probe is closed again (and stays closed).
    pub fn connect_target(&mut self, opts: ConnectOptions) -> Result<DeviceSession> {
        let r = self.connect_inner(opts);
        if r.is_err() {
            self.disconnect();
        }
        r
    }

    /// «Отключить»: close the USB device (drop `StLink` → leave debug mode,
    /// release the interface) so other programs can use the probe.
    pub fn disconnect(&mut self) {
        if self.link.take().is_some() {
            info!("probe {} released", self.info.id);
        }
        self.opts = None;
        self.target = None;
    }

    fn connect_inner(&mut self, opts: ConnectOptions) -> Result<DeviceSession> {
        self.target = None;
        let link = self.open()?;
        let conn = Self::connect(link, &opts)?;
        let target = link.identify()?;
        let rdp = match target.chip {
            Some(chip) => link.read_protection(chip).unwrap_or_else(|e| {
                warn!("cannot read RDP level: {e}");
                None
            }),
            None => None,
        };
        info!(chip = target.chip.map(|c| c.dev_type.as_str()), "identified");
        let session = DeviceSession {
            probe: self.info.clone(),
            connection: ConnectionDto::from(&conn),
            target: TargetInfoDto::new(&target, rdp),
        };
        self.opts = Some(opts);
        self.target = Some(target);
        Ok(session)
    }

    /// Pure planning against the identified target.
    pub fn plan_write(&mut self, addr: u32, len: u64) -> Result<FlashPlan> {
        let target = self
            .target
            .as_ref()
            .ok_or_else(|| BackendError::other("target not identified"))?;
        let len = u32::try_from(len)
            .map_err(|_| BackendError::other("invalid argument: data does not fit into flash"))?;
        let plan = flash::plan_write(target, addr, len as usize)?;
        Ok(FlashPlan::new(&plan, len))
    }

    pub fn write(
        &mut self,
        job: WriteJob,
        cancel: &AtomicBool,
        mut on_progress: impl FnMut(ProgressDto),
    ) -> Result<WriteResult> {
        let data = files::read_bin(&job.file_id)?;
        let started = Instant::now();
        let r = (|| {
            let target = self.current_target()?;
            let len = data.len() as u32;
            let plan = flash::plan_write(&target, job.addr, data.len())?;
            let link = self.link()?;
            let mut timer = StageTimer::new();
            link.write_flash(&target, job.addr, &data, job.verify, |p| {
                timer.on(p);
                on_progress(p.into());
                check_cancel(cancel)
            })?;
            let stage_ms = timer.finish();
            if job.reset_after {
                link.reset_and_run()?;
            }
            Ok(WriteResult {
                plan: FlashPlan::new(&plan, len),
                crc32: flash::crc32(&data),
                elapsed_ms: ms(started),
                stage_ms,
                verified: job.verify,
                reset_done: job.reset_after,
            })
        })();
        self.after(r)
    }

    pub fn read(
        &mut self,
        job: ReadJob,
        cancel: &AtomicBool,
        mut on_progress: impl FnMut(ProgressDto),
    ) -> Result<ReadResult> {
        let path = job
            .target
            .path
            .clone()
            .ok_or_else(|| BackendError::other("no file to save to"))?;
        let started = Instant::now();
        let r = (|| {
            let target = self.link()?.identify()?;
            let data = self.link()?.dump(&target, job.addr, Some(job.size), |p| {
                on_progress(p.into());
                check_cancel(cancel)
            })?;
            Ok(data)
        })();
        let data = self.after(r)?;
        std::fs::write(&path, &data)
            .map_err(|e| BackendError::other(format!("cannot write {path}: {e}")))?;
        info!("read {} bytes from 0x{:08X} → {path}", data.len(), job.addr);
        Ok(ReadResult {
            addr: job.addr,
            size: job.size,
            crc32: flash::crc32(&data),
            elapsed_ms: ms(started),
            saved_to: job.target,
        })
    }

    pub fn reset_and_run(&mut self) -> Result<()> {
        let r = (|| Ok(self.link()?.reset_and_run()?))();
        self.after(r)
    }

    /// Erase the entire flash; the core stays halted.
    pub fn erase_all(
        &mut self,
        cancel: &AtomicBool,
        mut on_progress: impl FnMut(ProgressDto),
    ) -> Result<EraseResult> {
        let started = Instant::now();
        let r = (|| {
            let target = self.current_target()?;
            let mut bank_ms = Vec::new();
            let mut bank_started = Instant::now();
            let mut banks = 0;
            self.link()?.mass_erase(&target, |p| {
                if p.done > 0 {
                    bank_ms.push(ms(bank_started));
                    bank_started = Instant::now();
                }
                banks = p.total as u8;
                on_progress(p.into());
                check_cancel(cancel)
            })?;
            let size = target.flash_size.unwrap_or(0);
            info!("flash erased: {banks} bank(s), {size} bytes in {:.0} ms", ms(started));
            Ok(EraseResult {
                banks,
                size,
                elapsed_ms: ms(started),
                bank_ms,
            })
        })();
        self.after(r)
    }
}

fn check_cancel(cancel: &AtomicBool) -> ControlFlow<()> {
    if cancel.load(Ordering::Relaxed) {
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// Time spent in each write stage, from the progress callbacks.
struct StageTimer {
    current: Option<(Stage, Instant)>,
    ms: StageMs,
}

impl StageTimer {
    fn new() -> Self {
        Self {
            current: None,
            ms: StageMs::default(),
        }
    }

    fn on(&mut self, p: Progress) {
        if self.current.is_some_and(|(s, _)| s == p.stage) {
            return;
        }
        self.close();
        self.current = Some((p.stage, Instant::now()));
    }

    fn close(&mut self) {
        if let Some((stage, t)) = self.current.take() {
            let slot = match stage {
                Stage::Erasing => &mut self.ms.erasing,
                Stage::Programming => &mut self.ms.programming,
                Stage::Verifying => &mut self.ms.verifying,
                Stage::Reading => return,
            };
            *slot += ms(t);
        }
    }

    fn finish(mut self) -> StageMs {
        self.close();
        self.ms
    }
}

/// Cancellation flags of running operations, keyed by the frontend's op id.
/// A cancel that arrives before the operation registered leaves a set flag.
#[derive(Default)]
pub struct CancelFlags(std::sync::Mutex<std::collections::HashMap<String, Arc<AtomicBool>>>);

impl CancelFlags {
    pub fn register(&self, op_id: &str) -> Arc<AtomicBool> {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(op_id.to_owned()).or_default().clone()
    }

    pub fn cancel(&self, op_id: &str) {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(op_id.to_owned())
            .or_default()
            .store(true, Ordering::Relaxed);
    }

    pub fn remove(&self, op_id: &str) {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        m.remove(op_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_cancel_is_seen_by_register() {
        let flags = CancelFlags::default();
        flags.cancel("op1");
        assert!(flags.register("op1").load(Ordering::Relaxed));
        flags.remove("op1");
        assert!(!flags.register("op1").load(Ordering::Relaxed));
    }

    /// Bench test (ST-Link + H743): `cargo test -p stlink-rust-ui -- --ignored`.
    /// While connected the probe is ours; after `disconnect` st-info and
    /// OpenOCD can use it, and connecting again works.
    #[test]
    #[ignore = "needs an ST-Link with a target, st-info and openocd"]
    fn hw_disconnect_frees_the_probe() {
        let desc = stlink::list_probes().unwrap_or_default().into_iter().next();
        assert!(desc.is_some(), "no ST-Link connected");
        let Some(desc) = desc else { return };
        let tool_ok = |cmd: &str, args: &[&str]| {
            std::process::Command::new(cmd)
                .args(args)
                .output()
                .is_ok_and(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).contains("Found 0"))
        };
        let st_info = || tool_ok("st-info", &["--probe"]);
        let openocd = || {
            tool_ok(
                "openocd",
                &["-f", "interface/stlink.cfg", "-f", "target/stm32h7x.cfg", "-c", "init; shutdown"],
            )
        };
        let opts = ConnectOptions {
            under_reset: true,
            ..ConnectOptions::default()
        };

        let mut w = Worker::new(desc);
        let s = w.connect_target(opts);
        assert!(matches!(&s, Ok(s) if s.target.chip_id == 0x450), "{s:?}");
        assert!(!openocd(), "openocd must not get the probe while we hold it");

        w.disconnect();
        assert!(st_info(), "st-info --probe after disconnect");
        assert!(openocd(), "openocd after disconnect");

        let again = w.connect_target(opts);
        assert!(again.is_ok(), "{again:?}");
        w.disconnect();
    }

    #[test]
    fn stage_timer_sums_stages() {
        let mut t = StageTimer::new();
        for stage in [Stage::Erasing, Stage::Erasing, Stage::Programming] {
            t.on(Progress {
                stage,
                done: 0,
                total: 1,
            });
        }
        let ms = t.finish();
        assert!(ms.erasing >= 0.0 && ms.programming >= 0.0);
        assert_eq!(ms.verifying, 0.0);
    }
}
