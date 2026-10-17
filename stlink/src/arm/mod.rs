//! ARM debug level: memory access, Cortex-M core identification and control.

use std::fmt;
use std::thread;
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use crate::probe::{NrstAction, ProbeCapabilities, StLink};
use crate::{Error, Result};

/// CPUID register.
pub const CPUID: u32 = 0xE000_ED00;
/// Debug Halting Control and Status Register.
pub const DHCSR: u32 = 0xE000_EDF0;

const DBGKEY: u32 = 0xA05F << 16;
const C_DEBUGEN: u32 = 1 << 0;
const C_HALT: u32 = 1 << 1;
const S_HALT: u32 = 1 << 17;
const S_RESET_ST: u32 = 1 << 25;
/// Debug Exception and Monitor Control Register.
const DEMCR: u32 = 0xE000_EDFC;
const VC_CORERESET: u32 = 1 << 0;
/// Application Interrupt and Reset Control Register.
const AIRCR: u32 = 0xE000_ED0C;
const AIRCR_VECTKEY: u32 = 0x05FA << 16;
const SYSRESETREQ: u32 = 1 << 2;
/// DEMCR bits that always read as zero (ARMv7-M).
const DEMCR_RESERVED: u32 = 0xFEF0_F80E;
/// DHCSR bits that always read as zero (ARMv7-M/ARMv8-M). A read with any of
/// them set did not come from the core (e.g. stale data from a dead AP).
const DHCSR_RESERVED: u32 = 0xF0F0_FFC0;

const ARM_IMPLEMENTER: u8 = 0x41;
const HALT_TIMEOUT: Duration = Duration::from_millis(500);
const RESET_HALT_TIMEOUT: Duration = Duration::from_millis(200);
const WAIT_RETRIES: usize = 3;

/// Cortex-M core type from CPUID PARTNO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreType {
    M0,
    M0Plus,
    M3,
    M4,
    M7,
    M23,
    M33,
    Unknown(u16),
}

impl CoreType {
    fn from_partno(partno: u16) -> Self {
        match partno {
            0xC20 => CoreType::M0,
            0xC60 => CoreType::M0Plus,
            0xC23 => CoreType::M3,
            0xC24 => CoreType::M4,
            0xC27 => CoreType::M7,
            0xD20 => CoreType::M23,
            0xD21 => CoreType::M33,
            other => CoreType::Unknown(other),
        }
    }
}

impl fmt::Display for CoreType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreType::M0 => f.write_str("Cortex-M0"),
            CoreType::M0Plus => f.write_str("Cortex-M0+"),
            CoreType::M3 => f.write_str("Cortex-M3"),
            CoreType::M4 => f.write_str("Cortex-M4"),
            CoreType::M7 => f.write_str("Cortex-M7"),
            CoreType::M23 => f.write_str("Cortex-M23"),
            CoreType::M33 => f.write_str("Cortex-M33"),
            CoreType::Unknown(p) => write!(f, "unknown core (PARTNO 0x{p:03X})"),
        }
    }
}

/// Decoded CPUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreInfo {
    pub cpuid: u32,
    pub implementer: u8,
    pub core: CoreType,
    /// `(variant, revision)` → `rVpR`.
    pub revision: (u8, u8),
}

impl CoreInfo {
    pub fn from_cpuid(cpuid: u32) -> Self {
        Self {
            cpuid,
            implementer: (cpuid >> 24) as u8,
            core: CoreType::from_partno(((cpuid >> 4) & 0xFFF) as u16),
            revision: (((cpuid >> 20) & 0xF) as u8, (cpuid & 0xF) as u8),
        }
    }

    pub fn is_arm(&self) -> bool {
        self.implementer == ARM_IMPLEMENTER
    }
}

impl fmt::Display for CoreInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} r{}p{} (CPUID 0x{:08X})",
            self.core, self.revision.0, self.revision.1, self.cpuid
        )
    }
}

/// Core registers of a halted Cortex-M.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoreRegisters {
    pub r: [u32; 13],
    pub sp: u32,
    pub lr: u32,
    pub pc: u32,
    pub xpsr: u32,
    pub msp: u32,
    pub psp: u32,
}

impl StLink {
    /// Read memory of any alignment and length (8-bit head/tail, 32-bit body).
    pub fn read_memory(&mut self, addr: u32, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let max8 = self.capabilities().max_rw8;
        for (a, n, wide) in plan_chunks(addr, len, max8) {
            let chunk = retry_wait(|| {
                if wide {
                    self.read_mem32(a, n)
                } else {
                    self.read_mem8(a, n)
                }
            })?;
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    /// Read one word via `READMEM_32BIT` + last R/W status.
    ///
    /// Not `READ_DEBUG_REG`: on V2 firmware (seen on J46) it reports OK with
    /// stale data when the AP access fails, e.g. while the target is in reset.
    pub fn read_u32(&mut self, addr: u32) -> Result<u32> {
        let bytes = retry_wait(|| self.read_mem32(addr, 4))?;
        crate::probe::le_u32_at(&bytes, 0)
    }

    /// Write one word via `WRITEMEM_32BIT` + last R/W status (see [`StLink::read_u32`]).
    pub fn write_u32(&mut self, addr: u32, value: u32) -> Result<()> {
        retry_wait(|| self.write_mem32(addr, &value.to_le_bytes()))
    }

    /// Read one word through AP `ap` (AP ≠ 0 needs [`StLink::init_ap`] first).
    pub fn read_u32_ap(&mut self, ap: u8, addr: u32) -> Result<u32> {
        let bytes = retry_wait(|| self.read_mem32_ap(ap, addr, 4))?;
        crate::probe::le_u32_at(&bytes, 0)
    }

    /// Write one word through AP `ap` (AP ≠ 0 needs [`StLink::init_ap`] first).
    pub fn write_u32_ap(&mut self, ap: u8, addr: u32, value: u32) -> Result<()> {
        retry_wait(|| self.write_mem32_ap(ap, addr, &value.to_le_bytes()))
    }

    pub fn core_info(&mut self) -> Result<CoreInfo> {
        Ok(CoreInfo::from_cpuid(self.read_u32(CPUID)?))
    }

    /// `true` only for a plausible DHCSR with S_HALT set.
    pub fn is_halted(&mut self) -> Result<bool> {
        Ok(dhcsr_halted(self.read_u32(DHCSR)?))
    }

    /// Halt the core and wait until it reports halted.
    pub fn halt(&mut self) -> Result<()> {
        self.write_u32(DHCSR, DBGKEY | C_DEBUGEN | C_HALT)?;
        let start = Instant::now();
        while !self.is_halted()? {
            if start.elapsed() > HALT_TIMEOUT {
                return Err(Error::Timeout);
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

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

    /// Resume the core (debug stays enabled).
    pub fn run(&mut self) -> Result<()> {
        self.write_u32(DHCSR, DBGKEY | C_DEBUGEN)
    }

    /// Read core registers; halts the core if needed and restores its run state.
    ///
    /// Returns the registers and whether the core was running before.
    pub fn read_core_registers(&mut self) -> Result<(CoreRegisters, bool)> {
        let dhcsr = self.read_u32(DHCSR)?;
        let was_running = dhcsr & S_HALT == 0;
        if was_running {
            self.halt()?;
        }
        let regs = self.read_halted_registers();
        if was_running {
            debug!("resuming core");
            self.run()?;
            if dhcsr & C_DEBUGEN == 0 {
                self.write_u32(DHCSR, DBGKEY)?;
            }
        }
        Ok((regs?, was_running))
    }

    fn read_halted_registers(&mut self) -> Result<CoreRegisters> {
        let mut regs = CoreRegisters::default();
        for (i, r) in (0u8..).zip(regs.r.iter_mut()) {
            *r = self.read_core_reg(i)?;
        }
        regs.sp = self.read_core_reg(13)?;
        regs.lr = self.read_core_reg(14)?;
        regs.pc = self.read_core_reg(15)?;
        regs.xpsr = self.read_core_reg(16)?;
        regs.msp = self.read_core_reg(17)?;
        regs.psp = self.read_core_reg(18)?;
        Ok(regs)
    }
}

fn dhcsr_halted(dhcsr: u32) -> bool {
    dhcsr & DHCSR_RESERVED == 0 && dhcsr & S_HALT != 0
}

/// Retry on AP/DP WAIT a few times.
fn retry_wait<T>(mut f: impl FnMut() -> Result<T>) -> Result<T> {
    let mut attempt = 0;
    loop {
        match f() {
            Err(e @ (Error::ApWait | Error::DpWait)) if attempt < WAIT_RETRIES => {
                attempt += 1;
                debug!("{e}, retry {attempt}/{WAIT_RETRIES}");
                thread::sleep(Duration::from_millis(1));
            }
            other => return other,
        }
    }
}

/// Split `[addr, addr+len)` into `(addr, len, is_32bit)` transfers:
/// unaligned head/tail as 8-bit, aligned body as 32-bit within 1 KiB windows.
fn plan_chunks(addr: u32, len: usize, max8: usize) -> Vec<(u32, usize, bool)> {
    let mut chunks = Vec::new();
    let mut a = u64::from(addr);
    let end = a + len as u64;
    let max32 = ProbeCapabilities::MAX_RW32 as u64;
    let max8 = max8.max(1) as u64;
    while a < end {
        let (n, wide) = if a % 4 == 0 && end - a >= 4 {
            let window_end = (a / max32 + 1) * max32;
            ((window_end.min(end) - a) & !3, true)
        } else {
            let next_aligned = (a / 4 + 1) * 4;
            let limit = if a % 4 == 0 {
                end
            } else {
                next_aligned.min(end)
            };
            ((limit - a).min(max8), false)
        };
        chunks.push((a as u32, n as usize, wide));
        a += n;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpuid_m7() {
        let c = CoreInfo::from_cpuid(0x411F_C271);
        assert!(c.is_arm());
        assert_eq!(c.core, CoreType::M7);
        assert_eq!(c.revision, (1, 1));
        assert_eq!(c.to_string(), "Cortex-M7 r1p1 (CPUID 0x411FC271)");
    }

    #[test]
    fn cpuid_others() {
        assert_eq!(CoreInfo::from_cpuid(0x410F_C241).core, CoreType::M4);
        assert_eq!(CoreInfo::from_cpuid(0x410C_C601).core, CoreType::M0Plus);
        assert_eq!(CoreInfo::from_cpuid(0x411F_C231).core, CoreType::M3);
        assert_eq!(CoreInfo::from_cpuid(0x410F_D213).core, CoreType::M33);
        assert!(!CoreInfo::from_cpuid(0).is_arm());
    }

    #[test]
    fn dhcsr_plausibility() {
        assert!(dhcsr_halted(0x0003_0003));
        assert!(!dhcsr_halted(0x0101_0000));
        // Stale DBGKEY write data seen from a dead AP on H7.
        assert!(!dhcsr_halted(0xA05F_0000));
        assert!(!dhcsr_halted(0xA05F_0003));
    }

    #[test]
    fn chunks_aligned() {
        assert_eq!(
            plan_chunks(0x0800_0000, 2048, 64),
            vec![(0x0800_0000, 1024, true), (0x0800_0400, 1024, true),]
        );
    }

    #[test]
    fn chunks_respect_1k_window() {
        assert_eq!(
            plan_chunks(0x2000_03F0, 32, 64),
            vec![(0x2000_03F0, 16, true), (0x2000_0400, 16, true),]
        );
    }

    #[test]
    fn chunks_unaligned() {
        assert_eq!(
            plan_chunks(0x1FFF_7A22, 2, 64),
            vec![(0x1FFF_7A22, 2, false)]
        );
        assert_eq!(
            plan_chunks(0x1001, 10, 64),
            vec![(0x1001, 3, false), (0x1004, 4, true), (0x1008, 3, false),]
        );
    }

    #[test]
    fn chunks_cover_exactly() {
        for (addr, len) in [(0u32, 0usize), (3, 1), (5, 4097), (0xFFFF_FFF0, 16)] {
            let chunks = plan_chunks(addr, len, 64);
            assert_eq!(chunks.iter().map(|c| c.1).sum::<usize>(), len);
            assert!(
                chunks
                    .iter()
                    .all(|&(a, n, wide)| !wide || (a % 4 == 0 && n % 4 == 0))
            );
            assert!(chunks.iter().all(|&(_, n, wide)| wide || n <= 64));
        }
    }
}
