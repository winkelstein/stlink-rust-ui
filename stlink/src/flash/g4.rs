//! STM32G4 flash controller (RM0440), driven by direct register access over
//! SWD. Pages of 2 KiB (4 KiB on G47x/G48x in single-bank mode), programmed
//! in 64-bit double words (two consecutive 32-bit writes). G47x/G48x: dual
//! bank when OPTR.DBANK is set. Option bytes are never touched.

use std::time::Duration;

use tracing::debug;

use super::FlashDriver;
use super::ctrl::{self, unlock_with_keys};
use super::geometry::{FlashGeometry, Sector};
use super::mem::Memory;
use crate::{Error, Result};

const REGS: u32 = 0x4002_2000;
const KEYR: u32 = REGS + 0x08;
const SR: u32 = REGS + 0x10;
const CR: u32 = REGS + 0x14;

const SR_EOP: u32 = 1 << 0;
const SR_OPERR: u32 = 1 << 1;
const SR_PROGERR: u32 = 1 << 3;
const SR_WRPERR: u32 = 1 << 4;
const SR_PGAERR: u32 = 1 << 5;
const SR_SIZERR: u32 = 1 << 6;
const SR_PGSERR: u32 = 1 << 7;
const SR_MISSERR: u32 = 1 << 8;
const SR_FASTERR: u32 = 1 << 9;
const SR_BSY: u32 = 1 << 16;
const SR_ERRORS: u32 =
    SR_OPERR | SR_PROGERR | SR_WRPERR | SR_PGAERR | SR_SIZERR | SR_PGSERR | SR_MISSERR | SR_FASTERR;

const CR_PG: u32 = 1 << 0;
const CR_PER: u32 = 1 << 1;
const CR_MER1: u32 = 1 << 2;
const CR_PNB_SHIFT: u32 = 3;
const CR_PNB_MASK: u32 = 0xFF;
const CR_BKER: u32 = 1 << 11;
const CR_MER2: u32 = 1 << 15;
const CR_STRT: u32 = 1 << 16;
const CR_LOCK: u32 = 1 << 31;

/// Datasheet: page erase ≤ 40 ms, mass erase ≤ 40 ms, double word ≤ 90 µs.
const PAGE_ERASE_TIMEOUT: Duration = Duration::from_secs(1);
const MASS_ERASE_TIMEOUT: Duration = Duration::from_secs(10);
const PROGRAM_TIMEOUT: Duration = Duration::from_millis(200);
/// Whole double words only.
const PROGRAM_CHUNK: usize = 1024;

pub(crate) struct G4Flash {
    geometry: FlashGeometry,
}

impl G4Flash {
    /// `geometry`: uniform pages in one or two banks, 8-byte granularity.
    pub(crate) fn new(geometry: FlashGeometry) -> Self {
        Self { geometry }
    }

    fn dual(&self) -> bool {
        self.geometry.bank_count() > 1
    }

    /// Clear stale flags before an operation (write 1 to clear).
    fn clear(m: &mut dyn Memory) -> Result<()> {
        m.write_u32(SR, SR_EOP | SR_ERRORS)
    }

    fn finish(m: &mut dyn Memory, timeout: Duration) -> Result<()> {
        let sr = ctrl::wait_idle(m, SR, SR_BSY, timeout)?;
        Self::clear(m)?;
        if sr & SR_ERRORS != 0 {
            debug!("flash error, SR 0x{sr:08X}");
        }
        ctrl::check_errors(sr, SR_ERRORS, SR_WRPERR)
    }
}

impl FlashDriver for G4Flash {
    fn geometry(&self) -> &FlashGeometry {
        &self.geometry
    }

    fn unlock(&mut self, m: &mut dyn Memory, _bank: u8) -> Result<()> {
        unlock_with_keys(m, KEYR, CR, CR_LOCK)?;
        ctrl::wait_idle(m, SR, SR_BSY, PROGRAM_TIMEOUT)?;
        Self::clear(m)
    }

    fn lock(&mut self, m: &mut dyn Memory, _bank: u8) -> Result<()> {
        m.write_u32(CR, CR_LOCK)
    }

    fn erase_sector(&mut self, m: &mut dyn Memory, page: &Sector) -> Result<()> {
        if page.index > CR_PNB_MASK {
            return Err(Error::InvalidArgument("page number out of range"));
        }
        Self::clear(m)?;
        let bker = if page.bank == 1 && self.dual() {
            CR_BKER
        } else {
            0
        };
        let cr = CR_PER | (page.index << CR_PNB_SHIFT) | bker;
        m.write_u32(CR, cr)?;
        m.write_u32(CR, cr | CR_STRT)?;
        let r = Self::finish(m, PAGE_ERASE_TIMEOUT);
        m.write_u32(CR, 0)?;
        r
    }

    fn erase_bank(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()> {
        // Single bank: MER1 erases everything.
        let mer = if bank == 1 && self.dual() {
            CR_MER2
        } else {
            CR_MER1
        };
        Self::clear(m)?;
        m.write_u32(CR, mer)?;
        m.write_u32(CR, mer | CR_STRT)?;
        let r = Self::finish(m, MASS_ERASE_TIMEOUT);
        m.write_u32(CR, 0)?;
        r
    }

    fn program(&mut self, m: &mut dyn Memory, addr: u32, data: &[u8]) -> Result<()> {
        self.geometry
            .bank_of(addr)
            .ok_or(Error::InvalidArgument("program address outside flash"))?;
        if addr % 8 != 0 || data.len() % 8 != 0 {
            return Err(Error::InvalidArgument(
                "G4 programs whole 64-bit double words",
            ));
        }
        Self::clear(m)?;
        m.write_u32(CR, CR_PG)?;
        let mut r = Ok(());
        let mut done = 0;
        while done < data.len() {
            let a = addr + done as u32;
            let n = (PROGRAM_CHUNK - (a as usize % PROGRAM_CHUNK)).min(data.len() - done);
            let block = &data[done..done + n];
            if block.iter().all(|&b| b == self.geometry.erased_value) {
                done += n;
                continue;
            }
            // Consecutive word pairs: each double word is two 32-bit writes in a row.
            r = m
                .write_block32(a, block)
                .and_then(|()| Self::finish(m, PROGRAM_TIMEOUT));
            if r.is_err() {
                debug!("programming failed in 0x{a:08X}..0x{:08X}", a as usize + n);
                break;
            }
            done += n;
        }
        m.write_u32(CR, 0)?;
        r
    }
}
