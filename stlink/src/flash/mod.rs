//! Flash operations: reading (dump), protection check, erase/program/verify.

mod geometry;
mod h7;

use std::ops::ControlFlow;

use tracing::{debug, info, warn};

use crate::chips::ChipDescriptor;
use crate::probe::StLink;
use crate::target::TargetInfo;
use crate::{Error, Result};

pub use geometry::{FlashGeometry, Sector, WritePlan};

/// Bytes programmed between progress callbacks.
const PROGRAM_BLOCK: usize = 4 * 1024;

/// Per-family flash controller.
pub(crate) trait FlashDriver {
    fn geometry(&self) -> FlashGeometry;
    fn unlock(&mut self, link: &mut StLink, bank: u8) -> Result<()>;
    fn lock(&mut self, link: &mut StLink, bank: u8) -> Result<()>;
    fn erase_sector(&mut self, link: &mut StLink, sector: &Sector) -> Result<()>;
    /// Program `data` at `addr`: aligned to the write granularity, one bank.
    fn program(&mut self, link: &mut StLink, addr: u32, data: &[u8]) -> Result<()>;
}

fn driver_for(chip: &ChipDescriptor, flash_size: u32) -> Result<Box<dyn FlashDriver>> {
    match chip.flash_type.as_str() {
        // H74x/H75x and H72x/H73x: uniform 128 KiB sectors (not H7Ax/H7Bx).
        "H7" if chip.flash_pagesize == 128 * 1024 => Ok(Box::new(h7::H7Flash::new(
            flash_size,
            chip.flash_pagesize,
            chip.dual_bank,
        ))),
        _ => Err(Error::Unsupported("flash programming for this chip family")),
    }
}

fn target_driver(target: &TargetInfo) -> Result<Box<dyn FlashDriver>> {
    let chip = target
        .chip
        .ok_or(Error::Unsupported("flash programming for an unknown chip"))?;
    let flash_size = target.flash_size.ok_or(Error::Unsupported(
        "flash programming without a known flash size",
    ))?;
    driver_for(chip, flash_size)
}

/// Flash layout of a supported target; `Unsupported` if there is no flash
/// driver for the chip (writing is not possible then).
pub fn geometry(target: &TargetInfo) -> Result<FlashGeometry> {
    Ok(target_driver(target)?.geometry())
}

/// Validate a write and list the sectors it will erase, without touching the
/// target (e.g. for a confirmation dialog).
pub fn plan_write(target: &TargetInfo, addr: u32, len: usize) -> Result<WritePlan> {
    target_driver(target)?.geometry().plan_write(addr, len)
}

/// Bytes to read for a dump at `addr`: `size` if given, otherwise up to the
/// end of flash (only when `addr` is inside flash).
pub fn read_len(target: &TargetInfo, addr: u32, size: Option<u32>) -> Result<usize> {
    let len = match (size, target.flash_end()) {
        (Some(size), _) => u64::from(size),
        (None, Some(end)) if target.in_flash(addr) => end - u64::from(addr),
        _ => {
            return Err(Error::InvalidArgument(
                "size is required outside the known flash range",
            ));
        }
    };
    if u64::from(addr) + len > 1 << 32 {
        return Err(Error::InvalidArgument(
            "range past the end of address space",
        ));
    }
    usize::try_from(len).map_err(|_| Error::InvalidArgument("size too large"))
}

/// Bytes read between progress callbacks.
const READ_BLOCK: usize = 16 * 1024;

/// Stage of a long operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Erasing,
    Programming,
    Verifying,
    Reading,
}

/// Progress report; the callback may return `ControlFlow::Break(())` to cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub done: u64,
    pub total: u64,
}

/// Read-out protection level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RdpLevel {
    /// No protection.
    Level0,
    /// Flash not readable through debug.
    Level1,
    /// Debug permanently disabled.
    Level2,
}

impl RdpLevel {
    /// Decode the RDP option byte (`0xAA` = 0, `0xCC` = 2, anything else = 1).
    pub fn from_byte(b: u8) -> Self {
        match b {
            0xAA => RdpLevel::Level0,
            0xCC => RdpLevel::Level2,
            _ => RdpLevel::Level1,
        }
    }
}

impl StLink {
    /// Dump memory (by default the whole flash, see [`read_len`]). Refuses to
    /// read RDP-protected flash. Works with the core running.
    pub fn dump(
        &mut self,
        target: &TargetInfo,
        addr: u32,
        size: Option<u32>,
        on_progress: impl FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<Vec<u8>> {
        let len = read_len(target, addr, size)?;
        if let Some(chip) = target.chip
            && target.in_flash(addr)
        {
            self.ensure_unprotected(chip)?;
        }
        self.read_flash(addr, len, on_progress)
    }

    /// Read `len` bytes from `addr` in blocks, reporting progress.
    /// Works with the core running (reads go through the AHB-AP).
    pub fn read_flash(
        &mut self,
        addr: u32,
        len: usize,
        mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<Vec<u8>> {
        if u64::from(addr) + len as u64 > 1 << 32 {
            return Err(Error::InvalidArgument(
                "range past the end of address space",
            ));
        }
        let total = len as u64;
        let mut out = Vec::with_capacity(len);
        let report = |done: usize, f: &mut dyn FnMut(Progress) -> ControlFlow<()>| {
            f(Progress {
                stage: Stage::Reading,
                done: done as u64,
                total,
            })
        };
        if report(0, &mut on_progress).is_break() {
            return Err(Error::Cancelled);
        }
        while out.len() < len {
            let n = (len - out.len()).min(READ_BLOCK);
            let block = self.read_memory(addr + out.len() as u32, n)?;
            out.extend_from_slice(&block);
            if report(out.len(), &mut on_progress).is_break() {
                debug!("read cancelled at {} of {len} bytes", out.len());
                return Err(Error::Cancelled);
            }
        }
        Ok(out)
    }

    /// Erase the touched sectors, program `data` at `addr`, optionally verify.
    ///
    /// The address/size are validated and RDP is checked before anything is
    /// erased. The core is halted and stays halted; the flash is locked again
    /// on success and on error. Cancelling is possible between blocks.
    pub fn write_flash(
        &mut self,
        target: &TargetInfo,
        addr: u32,
        data: &[u8],
        verify: bool,
        mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<()> {
        let mut driver = target_driver(target)?;
        let geometry = driver.geometry();
        let plan = geometry.plan_write(addr, data.len())?;
        if let Some(chip) = target.chip {
            self.ensure_unprotected(chip)?;
        }
        let image = geometry::pad(data, plan.padded_len as usize, geometry.erased_value);
        info!(
            "write 0x{addr:08X}..0x{:08X}, {} sector(s)",
            u64::from(addr) + u64::from(plan.padded_len),
            plan.sectors.len()
        );

        self.halt()?;
        let mut banks: Vec<u8> = plan.sectors.iter().map(|s| s.bank).collect();
        banks.dedup();
        let result = (|| {
            for &bank in &banks {
                driver.unlock(self, bank)?;
            }
            self.erase_and_program(driver.as_mut(), &plan, &image, &mut on_progress)
        })();
        for &bank in &banks {
            if let Err(e) = driver.lock(self, bank) {
                warn!("cannot lock flash bank {}: {e}", bank + 1);
            }
        }
        result?;

        if verify {
            self.verify_flash(addr, &image, &mut on_progress)?;
        }
        Ok(())
    }

    fn erase_and_program(
        &mut self,
        driver: &mut dyn FlashDriver,
        plan: &WritePlan,
        image: &[u8],
        on_progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<()> {
        let geometry = driver.geometry();
        let step =
            |stage, done: usize, total: usize, f: &mut dyn FnMut(Progress) -> ControlFlow<()>| {
                let p = Progress {
                    stage,
                    done: done as u64,
                    total: total as u64,
                };
                if f(p).is_break() {
                    Err(Error::Cancelled)
                } else {
                    Ok(())
                }
            };

        let n = plan.sectors.len();
        step(Stage::Erasing, 0, n, on_progress)?;
        for (i, sector) in plan.sectors.iter().enumerate() {
            debug!(
                "erase bank {} sector {} @0x{:08X}",
                sector.bank + 1,
                sector.index,
                sector.addr
            );
            driver.erase_sector(self, sector)?;
            step(Stage::Erasing, i + 1, n, on_progress)?;
        }

        let total = image.len();
        step(Stage::Programming, 0, total, on_progress)?;
        let mut done = 0;
        while done < total {
            let a = plan.addr + done as u32;
            // Never cross a bank boundary within one program call.
            let bank_end =
                geometry.base + (u32::from(geometry.bank_of(a)) + 1) * geometry.bank_size;
            let n = (total - done)
                .min(PROGRAM_BLOCK)
                .min((bank_end - a) as usize);
            driver.program(self, a, &image[done..done + n])?;
            done += n;
            step(Stage::Programming, done, total, on_progress)?;
        }
        Ok(())
    }

    fn verify_flash(
        &mut self,
        addr: u32,
        image: &[u8],
        on_progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<()> {
        let total = image.len();
        let start = Progress {
            stage: Stage::Verifying,
            done: 0,
            total: total as u64,
        };
        if on_progress(start).is_break() {
            return Err(Error::Cancelled);
        }
        let mut done = 0;
        while done < total {
            let n = (total - done).min(READ_BLOCK);
            let a = addr + done as u32;
            let got = self.read_memory(a, n)?;
            if let Some(i) = got
                .iter()
                .zip(&image[done..done + n])
                .position(|(g, w)| g != w)
            {
                return Err(Error::Verify { addr: a + i as u32 });
            }
            done += n;
            let p = Progress {
                stage: Stage::Verifying,
                done: done as u64,
                total: total as u64,
            };
            if on_progress(p).is_break() {
                return Err(Error::Cancelled);
            }
        }
        Ok(())
    }

    /// `FlashProtected` unless RDP is level 0 (or unknown for this family).
    fn ensure_unprotected(&mut self, chip: &ChipDescriptor) -> Result<()> {
        match self.read_protection(chip)? {
            Some(RdpLevel::Level0) | None => Ok(()),
            Some(level) => {
                warn!("RDP {level:?}: flash not accessible over SWD");
                Err(Error::FlashProtected)
            }
        }
    }

    /// Current RDP level; `None` if the register is unknown for this family.
    /// Read only — option bytes are never modified.
    pub fn read_protection(&mut self, chip: &ChipDescriptor) -> Result<Option<RdpLevel>> {
        let Some((addr, shift)) = chip.rdp_register() else {
            return Ok(None);
        };
        let reg = self.read_u32(addr)?;
        let level = RdpLevel::from_byte((reg >> shift) as u8);
        debug!("RDP register 0x{addr:08X} = 0x{reg:08X} → {level:?}");
        Ok(Some(level))
    }
}

/// CRC-32 (IEEE 802.3, as zlib/`crc32` tools).
pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            table[i] = c;
            i += 1;
        }
        table
    };
    !data.iter().fold(!0u32, |c, &b| {
        TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arm::CoreInfo;
    use crate::chips;

    fn target(chip_id: u16, flash_size: Option<u32>) -> TargetInfo {
        TargetInfo {
            core: CoreInfo::from_cpuid(0x411F_C271),
            chip: chips::find(chip_id),
            chip_id,
            idcode_raw: u32::from(chip_id),
            flash_size,
            uid: None,
        }
    }

    #[test]
    fn read_len_defaults_to_end_of_flash() {
        let t = target(0x450, Some(2 * 1024 * 1024));
        assert_eq!(read_len(&t, 0x0800_0000, None).ok(), Some(2 * 1024 * 1024));
        assert_eq!(read_len(&t, 0x0810_0000, None).ok(), Some(1024 * 1024));
        assert_eq!(read_len(&t, 0x2000_0000, Some(64)).ok(), Some(64));
        assert!(
            read_len(&t, 0x2000_0000, None).is_err(),
            "outside flash needs size"
        );
        assert!(read_len(&t, 0xFFFF_FFF0, Some(32)).is_err(), "past 4 GiB");
        let unknown_size = target(0x450, None);
        assert!(read_len(&unknown_size, 0x0800_0000, None).is_err());
    }

    #[test]
    fn in_flash_bounds() {
        let t = target(0x450, Some(2 * 1024 * 1024));
        assert!(t.in_flash(0x0800_0000));
        assert!(t.in_flash(0x081F_FFFF));
        assert!(!t.in_flash(0x0820_0000));
        assert!(!t.in_flash(0x07FF_FFFF));
    }

    #[test]
    fn plan_write_needs_a_supported_chip() {
        let h743 = target(0x450, Some(2 * 1024 * 1024));
        let plan = plan_write(&h743, 0x0800_0000, 16892);
        assert_eq!(plan.map(|p| p.sectors.len()).ok(), Some(1));
        // Unknown chip, unknown size, unsupported family (H7Ax: 8 KiB sectors).
        assert!(plan_write(&target(0x123, Some(1 << 20)), 0x0800_0000, 32).is_err());
        assert!(plan_write(&target(0x450, None), 0x0800_0000, 32).is_err());
        assert!(plan_write(&target(0x480, Some(2 << 20)), 0x0800_0000, 32).is_err());
        // F427 is not implemented yet.
        assert!(matches!(
            plan_write(&target(0x419, Some(2 << 20)), 0x0800_0000, 32),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn rdp_decoding() {
        assert_eq!(RdpLevel::from_byte(0xAA), RdpLevel::Level0);
        assert_eq!(RdpLevel::from_byte(0xCC), RdpLevel::Level2);
        assert_eq!(RdpLevel::from_byte(0xBB), RdpLevel::Level1);
        assert_eq!(RdpLevel::from_byte(0x00), RdpLevel::Level1);
    }
}
