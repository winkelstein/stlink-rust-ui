//! Flash operations: reading (dump), protection check, erase/program/verify.

mod geometry;
mod h7;

use std::ops::ControlFlow;

use tracing::{debug, info, warn};

use crate::chips::{self, ChipDescriptor};
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
    /// Erase the whole bank (user flash only, never option bytes).
    fn erase_bank(&mut self, link: &mut StLink, bank: u8) -> Result<()>;
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
/// end of flash. The whole range must lie in flash (`OutOfFlash` otherwise);
/// other memory is read with `read_memory`.
pub fn read_len(target: &TargetInfo, addr: u32, size: Option<u32>) -> Result<usize> {
    let end = target.flash_end().ok_or(Error::Unsupported(
        "flash read without a known flash size (use `mem`)",
    ))?;
    let len = match size {
        Some(size) => u64::from(size),
        None if target.in_flash(addr) => end - u64::from(addr),
        None => 0,
    };
    if !target.in_flash(addr) || u64::from(addr) + len > end {
        return Err(Error::OutOfFlash {
            addr,
            len,
            start: chips::FLASH_BASE,
            end,
        });
    }
    if len == 0 {
        return Err(Error::InvalidArgument("nothing to read"));
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
    /// Dump flash (by default all of it, see [`read_len`]; the range must lie
    /// in flash). Refuses to read RDP-protected flash. Works with the core running.
    pub fn dump(
        &mut self,
        target: &TargetInfo,
        addr: u32,
        size: Option<u32>,
        on_progress: impl FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<Vec<u8>> {
        let len = read_len(target, addr, size)?;
        if let Some(chip) = target.chip {
            self.ensure_unprotected(chip)?;
        }
        self.read_flash(addr, len, on_progress)
    }

    /// Read `len` bytes from `addr` in blocks, reporting progress. Private:
    /// the range is checked by [`StLink::dump`]. Works with the core running.
    fn read_flash(
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

    /// Erase the whole flash, bank by bank (progress: `Erasing`, done/total in
    /// banks). Checks RDP first; halts the core, which stays halted. Cancelling
    /// is possible between banks. Option bytes are never touched.
    pub fn mass_erase(
        &mut self,
        target: &TargetInfo,
        mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<()> {
        let mut driver = target_driver(target)?;
        let banks = bank_count(&driver.geometry());
        if let Some(chip) = target.chip {
            self.ensure_unprotected(chip)?;
        }
        info!("mass erase: {banks} bank(s)");
        self.halt()?;
        erase_banks(banks, &mut on_progress, |bank| {
            driver.unlock(self, bank)?;
            let r = driver.erase_bank(self, bank);
            if let Err(e) = driver.lock(self, bank) {
                warn!("cannot lock flash bank {}: {e}", bank + 1);
            }
            debug!("bank {} erased", bank + 1);
            r
        })
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

fn bank_count(g: &FlashGeometry) -> u8 {
    (g.size / g.bank_size.max(1)).clamp(1, u32::from(u8::MAX)) as u8
}

/// Erase banks `0..banks` in order, reporting progress before the first and
/// after each bank; a `Break` stops before the next bank.
fn erase_banks(
    banks: u8,
    on_progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
    mut erase: impl FnMut(u8) -> Result<()>,
) -> Result<()> {
    let report = |done: u8, f: &mut dyn FnMut(Progress) -> ControlFlow<()>| {
        let p = Progress {
            stage: Stage::Erasing,
            done: u64::from(done),
            total: u64::from(banks),
        };
        if f(p).is_break() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    };
    report(0, on_progress)?;
    for bank in 0..banks {
        erase(bank)?;
        report(bank + 1, on_progress)?;
    }
    Ok(())
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
        let unknown_size = target(0x450, None);
        assert!(matches!(
            read_len(&unknown_size, 0x0800_0000, None),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn read_len_stays_in_flash() {
        let t = target(0x450, Some(2 * 1024 * 1024));
        let out = |r: Result<usize>| matches!(r, Err(Error::OutOfFlash { .. }));
        // Exactly up to the end of flash.
        assert_eq!(
            read_len(&t, 0x0800_0000, Some(0x20_0000)).ok(),
            Some(0x20_0000)
        );
        assert_eq!(read_len(&t, 0x081F_FFFC, Some(4)).ok(), Some(4));
        // One byte more.
        assert!(out(read_len(&t, 0x0800_0000, Some(0x20_0001))));
        assert!(out(read_len(&t, 0x081F_FFFC, Some(5))));
        // Inside bank 2.
        assert_eq!(
            read_len(&t, 0x0810_0000, Some(0x10_0000)).ok(),
            Some(0x10_0000)
        );
        assert!(out(read_len(&t, 0x0810_0000, Some(0x10_0001))));
        // Before and after flash, RAM, end of the address space.
        assert!(out(read_len(&t, 0x07FF_FFFC, Some(4))));
        assert!(out(read_len(&t, 0x0820_0000, None)));
        assert!(out(read_len(&t, 0x2000_0000, Some(64))));
        assert!(out(read_len(&t, 0xFFFF_FFF0, Some(32))));
        assert!(matches!(
            read_len(&t, 0x0800_0000, Some(0)),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn out_of_flash_message() {
        let e = Error::OutOfFlash {
            addr: 0x0800_0000,
            len: 0x20_0001,
            start: 0x0800_0000,
            end: 0x0820_0000,
        };
        assert_eq!(
            e.to_string(),
            "range 0x08000000+0x200001 is outside flash (0x08000000..0x08200000); use `mem` for other memory"
        );
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
    fn erase_banks_in_order_with_progress() {
        let mut erased = Vec::new();
        let mut seen = Vec::new();
        let r = erase_banks(
            2,
            &mut |p| {
                seen.push((p.done, p.total));
                ControlFlow::Continue(())
            },
            |b| {
                erased.push(b);
                Ok(())
            },
        );
        assert!(r.is_ok());
        assert_eq!(erased, [0, 1]);
        assert_eq!(seen, [(0, 2), (1, 2), (2, 2)]);
    }

    #[test]
    fn erase_banks_cancel_between_banks() {
        let mut erased = Vec::new();
        let r = erase_banks(
            2,
            &mut |p| {
                if p.done == 1 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
            |b| {
                erased.push(b);
                Ok(())
            },
        );
        assert!(matches!(r, Err(Error::Cancelled)));
        assert_eq!(erased, [0]);
    }

    #[test]
    fn erase_banks_stops_on_error() {
        let mut calls = 0;
        let r = erase_banks(2, &mut |_| ControlFlow::Continue(()), |_| {
            calls += 1;
            Err(Error::FlashOperation { status: 0x20000 })
        });
        assert!(matches!(r, Err(Error::FlashOperation { .. })));
        assert_eq!(calls, 1);
    }

    #[test]
    fn bank_count_from_geometry() {
        let g = FlashGeometry {
            base: 0x0800_0000,
            size: 2 << 20,
            bank_size: 1 << 20,
            sector_size: 128 << 10,
            write_granularity: 32,
            erased_value: 0xFF,
        };
        assert_eq!(bank_count(&g), 2);
        assert_eq!(
            bank_count(&FlashGeometry {
                bank_size: 2 << 20,
                ..g
            }),
            1
        );
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
