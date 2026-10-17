//! Target identification (the `st-info --probe` equivalent).

use tracing::{debug, info, warn};

use crate::arm::{CoreInfo, CoreType};
use crate::chips::{self, ChipDescriptor};
use crate::probe::StLink;
use crate::{Error, Result};

/// What `probe` found on the SWD bus.
#[derive(Debug, Clone)]
pub struct TargetInfo {
    pub core: CoreInfo,
    /// `None` if the chip id is not in the table.
    pub chip: Option<&'static ChipDescriptor>,
    /// DBGMCU_IDCODE[11:0] (after errata fix-ups).
    pub chip_id: u16,
    /// Full DBGMCU_IDCODE (REV_ID in bits 31:16).
    pub idcode_raw: u32,
    /// Flash size in bytes, if the chip is known.
    pub flash_size: Option<u32>,
    pub uid: Option<[u8; 12]>,
}

impl TargetInfo {
    pub fn rev_id(&self) -> u16 {
        (self.idcode_raw >> 16) as u16
    }
}

impl StLink {
    /// Identify the attached target. Requires [`StLink::enter_swd`]; does not halt the core.
    pub fn identify(&mut self) -> Result<TargetInfo> {
        let core = self.core_info()?;
        debug!(%core, "CPUID");
        if !core.is_arm() {
            return Err(match self.identify_via_debug_ap()? {
                Some(chip) => Error::CoreNotAccessible(chip.dev_type.clone()),
                None => Error::TargetNotConnected,
            });
        }

        let (chip_id, idcode_raw) = self.read_chip_id(core.core)?;
        let chip = chips::find(chip_id);
        info!(
            chip_id = format_args!("0x{chip_id:03x}"),
            dev_type = chip.map(|c| c.dev_type.as_str()),
            "chip"
        );

        let mut flash_size = None;
        let mut uid = None;
        if let Some(chip) = chip {
            // Not fatal: identification is still useful without these.
            match self.read_u16(chip.flash_size_reg) {
                Ok(reg) => flash_size = Some(chip.flash_size_quirk.flash_size(reg)),
                Err(e) => warn!("flash size register 0x{:08X}: {e}", chip.flash_size_reg),
            }
            if let Some(addr) = chip.uid_reg {
                match self.read_memory(addr, 12) {
                    Ok(bytes) => uid = bytes.try_into().ok(),
                    Err(e) => warn!("UID 0x{addr:08X}: {e}"),
                }
            }
        }
        Ok(TargetInfo {
            core,
            chip,
            chip_id,
            idcode_raw,
            flash_size,
            uid,
        })
    }

    /// Find the chip through a debug AP (H7: APB-D on AP2) without using the
    /// core's AP0. `None` if the firmware has no multi-AP support or nothing matches.
    pub fn identify_via_debug_ap(&mut self) -> Result<Option<&'static ChipDescriptor>> {
        if !self.capabilities().multi_ap {
            return Ok(None);
        }
        for &(ap, addr) in chips::debug_ap_idcode_addresses() {
            let raw = match self.init_ap(ap).and_then(|()| self.read_u32_ap(ap, addr)) {
                Ok(v) => v,
                Err(e @ (Error::Disconnected | Error::Timeout)) => return Err(e),
                Err(e) => {
                    debug!("AP{ap} IDCODE @0x{addr:08X}: {e}");
                    continue;
                }
            };
            debug!("AP{ap} IDCODE @0x{addr:08X} = 0x{raw:08X}");
            if let Some(chip) = chips::find((raw & 0xFFF) as u16) {
                return Ok(Some(chip));
            }
        }
        Ok(None)
    }

    /// Keep debug access in Sleep/Stop/Standby by setting DBGMCU_CR bits
    /// (persists until power-off). Returns `false` if unknown for this family.
    pub fn enable_debug_in_low_power(&mut self, chip: &ChipDescriptor) -> Result<bool> {
        let Some(d) = chip.dbgmcu_low_power() else {
            return Ok(false);
        };
        if d.ap != 0 {
            self.init_ap(d.ap)?;
        }
        let old = self.read_u32_ap(d.ap, d.addr)?;
        self.write_u32_ap(d.ap, d.addr, old | d.bits)?;
        info!(
            "DBGMCU_CR (AP{} 0x{:08X}): 0x{old:08X} → 0x{:08X}",
            d.ap,
            d.addr,
            old | d.bits
        );
        Ok(true)
    }

    /// Probe the candidate DBGMCU_IDCODE addresses for this core.
    fn read_chip_id(&mut self, core: CoreType) -> Result<(u16, u32)> {
        let mut first_nonzero = None;
        for &addr in chips::idcode_addresses(core) {
            let raw = match self.read_u32(addr) {
                Ok(v) => v,
                Err(e @ (Error::SwdFault(_) | Error::ApWait | Error::DpWait)) => {
                    debug!("IDCODE @0x{addr:08X}: {e}");
                    continue;
                }
                Err(e) => return Err(e),
            };
            debug!("IDCODE @0x{addr:08X} = 0x{raw:08X}");
            if raw == 0 {
                continue;
            }
            let mut id = (raw & 0xFFF) as u16;
            // F4 rev A errata: reports the F2 id (0x411) on a Cortex-M4.
            if id == 0x411 && core == CoreType::M4 {
                id = 0x413;
            }
            if chips::find(id).is_some() {
                return Ok((id, raw));
            }
            first_nonzero.get_or_insert((id, raw));
        }
        first_nonzero.ok_or(Error::UnknownChip(0))
    }

    /// 16-bit read through an aligned 32-bit access.
    fn read_u16(&mut self, addr: u32) -> Result<u16> {
        let word = self.read_u32(addr & !3)?;
        Ok((word >> ((addr & 2) * 8)) as u16)
    }
}
