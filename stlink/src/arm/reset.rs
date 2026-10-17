//! Target reset: connect under reset (NRST with AIRCR fallback) and
//! reset-and-run.

use std::thread;
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use super::{
    AIRCR, AIRCR_VECTKEY, C_DEBUGEN, C_HALT, DBGKEY, DEMCR, DEMCR_RESERVED, DHCSR, DHCSR_RESERVED,
    RESET_HALT_TIMEOUT, S_RESET_ST, SYSRESETREQ, VC_CORERESET, dhcsr_halted,
};
use crate::probe::{NrstAction, StLink};
use crate::{Error, Result};

impl StLink {
    /// Connect under reset: hold NRST low, arm halt + reset vector catch
    /// (DEMCR.VC_CORERESET), release NRST. The core stops at the first
    /// instruction, before the firmware can disable SWD or enter low-power
    /// mode. Needs NRST wired to the probe.
    ///
    /// The target is reset; on success the core is halted at the reset vector.
    pub fn attach_under_reset(&mut self) -> Result<()> {
        if !self.in_debug_mode() {
            self.enter_swd()?;
        }
        self.drive_nrst(NrstAction::Low)?;
        // Debug registers are reset only by power-on, so they can be armed
        // while NRST is held. Never read-modify-write here: on H7 the AP may be
        // dead in reset and return stale data with an OK status. Failures are
        // tolerated: we keep requesting halt below.
        let armed = self
            .write_u32(DHCSR, DBGKEY | C_DEBUGEN | C_HALT)
            .and_then(|()| self.write_u32(DEMCR, VC_CORERESET));
        if let Err(e) = &armed {
            debug!("arming halt under reset: {e}");
        }
        thread::sleep(Duration::from_millis(1));
        self.drive_nrst(NrstAction::High)?;

        let (mut halted, reset_seen) = self.wait_halt_after_reset()?;
        if halted && !reset_seen {
            // NRST had no effect (not wired to the probe): the core only halted
            // because of our DHCSR write. Fall back to a system reset, as stlink does.
            warn!("NRST did not reset the target (not wired?); using AIRCR.SYSRESETREQ");
            self.write_u32(DEMCR, VC_CORERESET)?;
            if let Err(e) = self.write_u32(AIRCR, AIRCR_VECTKEY | SYSRESETREQ) {
                // The reset may cut the transaction short.
                debug!("SYSRESETREQ write: {e}");
            }
            // Let the reset take effect before polling DHCSR.
            thread::sleep(Duration::from_millis(1));
            let (h, seen) = self.wait_halt_after_reset()?;
            if !seen {
                warn!("no reset seen after SYSRESETREQ either");
            }
            halted = h;
        }

        // Disarm the vector catch so later resets run normally. Once halted the
        // AP is alive and DEMCR can be read; anything implausible becomes 0.
        let demcr = if halted {
            self.read_u32(DEMCR)
                .ok()
                .filter(|v| v & DEMCR_RESERVED == 0)
        } else {
            None
        };
        let restored = demcr.map_or(0, |v| v & !VC_CORERESET);
        debug!("DEMCR read {demcr:08X?}, restoring 0x{restored:08X}");
        if let Err(e) = self.write_u32(DEMCR, restored) {
            debug!("restore DEMCR: {e}");
        }
        if halted { Ok(()) } else { Err(Error::Timeout) }
    }

    /// Poll DHCSR until a plausible halted state; also report whether a reset
    /// was seen (S_RESET_ST is sticky until read, and nothing read DHCSR before).
    fn wait_halt_after_reset(&mut self) -> Result<(bool, bool)> {
        let start = Instant::now();
        let mut reset_seen = false;
        loop {
            let dhcsr = self.read_u32(DHCSR)?;
            if dhcsr & DHCSR_RESERVED == 0 && dhcsr & S_RESET_ST != 0 {
                reset_seen = true;
            }
            if dhcsr_halted(dhcsr) {
                debug!(
                    "halted {:?} after reset (DHCSR 0x{dhcsr:08X}, reset seen: {reset_seen})",
                    start.elapsed()
                );
                return Ok((true, reset_seen));
            }
            if start.elapsed() > RESET_HALT_TIMEOUT {
                debug!("not halted after reset (DHCSR 0x{dhcsr:08X})");
                return Ok((false, reset_seen));
            }
            // Vector catch may not have been armed: request halt directly.
            if let Err(e) = self.write_u32(DHCSR, DBGKEY | C_DEBUGEN | C_HALT) {
                debug!("halt request: {e}");
            }
        }
    }

    /// System reset (AIRCR.SYSRESETREQ) and let the firmware run.
    pub fn reset_and_run(&mut self) -> Result<()> {
        // No reset vector catch, no pending halt: the core must run after reset.
        let demcr = self
            .read_u32(DEMCR)
            .ok()
            .filter(|v| v & DEMCR_RESERVED == 0)
            .unwrap_or(0);
        self.write_u32(DEMCR, demcr & !VC_CORERESET)?;
        self.run()?;
        if let Err(e) = self.write_u32(AIRCR, AIRCR_VECTKEY | SYSRESETREQ) {
            // The reset may cut the transaction short.
            debug!("SYSRESETREQ write: {e}");
        }
        Ok(())
    }
}
