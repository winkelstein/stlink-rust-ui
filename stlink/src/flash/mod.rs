//! Flash operations: reading (dump), protection check, erase/program/verify,
//! mass erase. Families: F0/F1/F3 (pages), F2/F4/F7 (sectors), G4 (pages),
//! H74x/H75x/H72x/H73x (sectors). Drivers talk to the target through the
//! [`mem::Memory`] trait, so the sequences are tested against a register
//! model (`fake`) without hardware.

mod ctrl;
#[cfg(test)]
mod driver_tests;
mod f0f1;
mod f4;
#[cfg(test)]
mod fake;
mod g4;
mod geometry;
mod h7;
mod layout;
pub(crate) mod mem;
mod ops;

use std::ops::ControlFlow;

use tracing::{debug, info, warn};

use crate::chips::{self, ChipDescriptor, RdpEncoding};
use crate::probe::StLink;
use crate::target::TargetInfo;
use crate::{Error, Result};

pub use geometry::{EraseUnit, FlashBank, FlashGeometry, Sector, SectorRun, WritePlan};

use mem::Memory;

/// Per-family flash controller.
pub(crate) trait FlashDriver {
    fn geometry(&self) -> &FlashGeometry;
    fn unlock(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()>;
    fn lock(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()>;
    /// Erase one page or sector.
    fn erase_sector(&mut self, m: &mut dyn Memory, sector: &Sector) -> Result<()>;
    /// Erase the whole bank (user flash only, never option bytes).
    fn erase_bank(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()>;
    /// Program `data` at `addr`: aligned to the write granularity, one bank.
    fn program(&mut self, m: &mut dyn Memory, addr: u32, data: &[u8]) -> Result<()>;
}

/// The driver of a family, with its geometry. `voltage` (target, volts)
/// picks the F2/F4/F7 programming parallelism.
fn driver_for(
    chip: &ChipDescriptor,
    flash_size: u32,
    dual_bank: Option<bool>,
    voltage: Option<f32>,
) -> Result<Box<dyn FlashDriver>> {
    let build = |unit, granularity| layout::build(chip, flash_size, dual_bank, unit, granularity);
    Ok(match chip.flash_type.as_str() {
        // H74x/H75x and H72x/H73x: uniform 128 KiB sectors (not H7Ax/H7Bx).
        "H7" if chip.flash_pagesize == 128 * 1024 => {
            Box::new(h7::H7Flash::new(build(EraseUnit::Sector, 32)?))
        }
        "F0_F1_F3" | "F1_XL" => Box::new(f0f1::F0F1Flash::new(build(EraseUnit::Page, 2)?)),
        "F2_F4" | "F7" => {
            let psize = f4::Psize::for_voltage(voltage);
            Box::new(f4::SectorFlash::new(
                build(EraseUnit::Sector, psize.granularity())?,
                psize,
            ))
        }
        "G4" => Box::new(g4::G4Flash::new(build(EraseUnit::Page, 8)?)),
        _ => return Err(Error::Unsupported("flash programming for this chip family")),
    })
}

fn target_driver(target: &TargetInfo, voltage: Option<f32>) -> Result<Box<dyn FlashDriver>> {
    let chip = target
        .chip
        .ok_or(Error::Unsupported("flash programming for an unknown chip"))?;
    let flash_size = target.flash_size.ok_or(Error::Unsupported(
        "flash programming without a known flash size",
    ))?;
    driver_for(chip, flash_size, target.dual_bank, voltage)
}

/// Flash layout of a supported target; `Unsupported` if there is no flash
/// driver for the chip (writing is not possible then).
pub fn geometry(target: &TargetInfo) -> Result<FlashGeometry> {
    Ok(target_driver(target, None)?.geometry().clone())
}

/// Validate a write and list the pages/sectors it will erase, without
/// touching the target (e.g. for a confirmation dialog).
pub fn plan_write(target: &TargetInfo, addr: u32, len: usize) -> Result<WritePlan> {
    target_driver(target, None)?
        .geometry()
        .plan_write(addr, len)
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

    /// Decode the protection register as stored by this family.
    pub fn decode(reg: u32, encoding: RdpEncoding) -> Self {
        match encoding {
            RdpEncoding::Byte { shift } => Self::from_byte((reg >> shift) as u8),
            RdpEncoding::F1Obr if reg & (1 << 1) != 0 => RdpLevel::Level1,
            RdpEncoding::F1Obr => RdpLevel::Level0,
            RdpEncoding::F0F3Obr => match (reg >> 1) & 0b11 {
                0 => RdpLevel::Level0,
                3 => RdpLevel::Level2,
                _ => RdpLevel::Level1,
            },
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
            let n = (len - out.len()).min(ops::READ_BLOCK);
            let block = self.read_memory(addr + out.len() as u32, n)?;
            out.extend_from_slice(&block);
            if report(out.len(), &mut on_progress).is_break() {
                debug!("read cancelled at {} of {len} bytes", out.len());
                return Err(Error::Cancelled);
            }
        }
        Ok(out)
    }

    /// The driver for `target`, with the parallelism for the current target
    /// voltage (F2/F4/F7).
    fn driver(&mut self, target: &TargetInfo) -> Result<Box<dyn FlashDriver>> {
        let voltage = match target.chip.map(|c| c.flash_type.as_str()) {
            Some("F2_F4" | "F7") => self.target_voltage().ok().flatten(),
            _ => None,
        };
        target_driver(target, voltage)
    }

    /// Erase the touched pages/sectors, program `data` at `addr`, optionally verify.
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
        let mut driver = self.driver(target)?;
        let plan = driver.geometry().plan_write(addr, data.len())?;
        if let Some(chip) = target.chip {
            self.ensure_unprotected(chip)?;
        }
        let image = geometry::pad(
            data,
            plan.padded_len as usize,
            driver.geometry().erased_value,
        );
        info!(
            "write 0x{addr:08X}..0x{:08X}, {} {}(s)",
            u64::from(addr) + u64::from(plan.padded_len),
            plan.sectors.len(),
            unit_name(driver.geometry().unit)
        );
        ops::write(
            self,
            driver.as_mut(),
            &plan,
            &image,
            verify,
            &mut on_progress,
        )
    }

    /// Erase the whole flash, bank by bank (progress: `Erasing`, done/total in
    /// banks). Checks RDP first; halts the core, which stays halted. Cancelling
    /// is possible between banks. Option bytes are never touched.
    pub fn mass_erase(
        &mut self,
        target: &TargetInfo,
        mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
    ) -> Result<()> {
        let mut driver = self.driver(target)?;
        if let Some(chip) = target.chip {
            self.ensure_unprotected(chip)?;
        }
        info!("mass erase: {} bank(s)", driver.geometry().bank_count());
        ops::mass_erase(self, driver.as_mut(), &mut on_progress)
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
        let Some(reg) = chip.rdp_register() else {
            return Ok(None);
        };
        let value = self.read_u32(reg.addr)?;
        let level = RdpLevel::decode(value, reg.encoding);
        debug!(
            "RDP register 0x{:08X} = 0x{value:08X} → {level:?}",
            reg.addr
        );
        Ok(Some(level))
    }
}

/// `page` / `sector`, for logs.
pub fn unit_name(unit: EraseUnit) -> &'static str {
    match unit {
        EraseUnit::Page => "page",
        EraseUnit::Sector => "sector",
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
            dual_bank: None,
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
        let r = ops::erase_banks(
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
        let r = ops::erase_banks(
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
        let r = ops::erase_banks(2, &mut |_| ControlFlow::Continue(()), |_| {
            calls += 1;
            Err(Error::FlashOperation { status: 0x20000 })
        });
        assert!(matches!(r, Err(Error::FlashOperation { .. })));
        assert_eq!(calls, 1);
    }

    #[test]
    fn bank_count_from_geometry() {
        let two = FlashGeometry::uniform(0x0800_0000, 2 << 20, 2, 128 << 10, EraseUnit::Sector, 32);
        assert_eq!(two.bank_count(), 2);
        let one = FlashGeometry::uniform(0x0800_0000, 2 << 20, 1, 128 << 10, EraseUnit::Sector, 32);
        assert_eq!(one.bank_count(), 1);
    }

    #[test]
    fn rdp_decoding_per_family() {
        let byte = RdpEncoding::Byte { shift: 8 };
        assert_eq!(RdpLevel::decode(0x0FFF_AAED, byte), RdpLevel::Level0);
        assert_eq!(RdpLevel::decode(0x0000_CC00, byte), RdpLevel::Level2);
        assert_eq!(RdpLevel::decode(0x0000_BB00, byte), RdpLevel::Level1);
        assert_eq!(
            RdpLevel::decode(0x03FF_FFFC, RdpEncoding::F1Obr),
            RdpLevel::Level0
        );
        assert_eq!(
            RdpLevel::decode(0x0000_0002, RdpEncoding::F1Obr),
            RdpLevel::Level1
        );
        // F1 bit 2 is WDG_SW, not part of RDP.
        assert_eq!(
            RdpLevel::decode(0x0000_0004, RdpEncoding::F1Obr),
            RdpLevel::Level0
        );
        assert_eq!(
            RdpLevel::decode(0b000, RdpEncoding::F0F3Obr),
            RdpLevel::Level0
        );
        assert_eq!(
            RdpLevel::decode(0b010, RdpEncoding::F0F3Obr),
            RdpLevel::Level1
        );
        assert_eq!(
            RdpLevel::decode(0b110, RdpEncoding::F0F3Obr),
            RdpLevel::Level2
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
