//! STM32F2/F4/F7 flash controller (RM0033, RM0090, RM0368, RM0385, RM0410,
//! RM0431), driven by direct register access over SWD. Sectors of different
//! sizes (layouts from `chips/layouts.txt`); one or two banks (F42x/F43x
//! DB1M or 2 MiB, F76x/F77x nDBANK). Programming parallelism x32, or x8 below
//! 2.7 V. Option bytes are never touched.

use std::time::Duration;

use tracing::{debug, warn};

use super::FlashDriver;
use super::ctrl::{self, unlock_with_keys};
use super::geometry::{FlashGeometry, Sector};
use super::mem::Memory;
use crate::{Error, Result};

const REGS: u32 = 0x4002_3C00;
const KEYR: u32 = REGS + 0x04;
const SR: u32 = REGS + 0x0C;
const CR: u32 = REGS + 0x10;

const SR_EOP: u32 = 1 << 0;
const SR_OPERR: u32 = 1 << 1;
const SR_WRPERR: u32 = 1 << 4;
const SR_PGAERR: u32 = 1 << 5;
const SR_PGPERR: u32 = 1 << 6;
/// PGSERR on F2/F4, ERSERR on F7.
const SR_PGSERR: u32 = 1 << 7;
const SR_BSY: u32 = 1 << 16;
const SR_ERRORS: u32 = SR_OPERR | SR_WRPERR | SR_PGAERR | SR_PGPERR | SR_PGSERR;

const CR_PG: u32 = 1 << 0;
const CR_SER: u32 = 1 << 1;
const CR_MER: u32 = 1 << 2;
const CR_SNB_SHIFT: u32 = 3;
const CR_PSIZE_SHIFT: u32 = 8;
/// MER1 (F42x/F43x) / MER2 (F76x/F77x): mass erase of bank 2.
const CR_MER_BANK2: u32 = 1 << 15;
const CR_STRT: u32 = 1 << 16;
const CR_LOCK: u32 = 1 << 31;

/// Datasheets (x32): 128 KiB sector ≤ 2 s (F4), 256 KiB ≤ 4 s (F7); at x8
/// roughly twice as long. Bank erase up to 16 s per MiB at x8.
const SECTOR_ERASE_TIMEOUT: Duration = Duration::from_secs(10);
const BANK_ERASE_TIMEOUT: Duration = Duration::from_secs(60);
const PROGRAM_TIMEOUT: Duration = Duration::from_millis(200);
const PROGRAM_CHUNK: usize = 1024;

/// Below this target voltage x32 programming is not allowed (RM0090 §3.6.2).
pub(crate) const MIN_X32_VOLTAGE: f32 = 2.7;

/// Programming parallelism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Psize {
    X8,
    X32,
}

impl Psize {
    /// x32 from 2.7 V; x8 below (or when the voltage is known to be low).
    pub(crate) fn for_voltage(voltage: Option<f32>) -> Self {
        match voltage {
            Some(v) if v > 0.5 && v < MIN_X32_VOLTAGE => {
                warn!(
                    "target voltage {v:.2} V < {MIN_X32_VOLTAGE} V: programming with x8 parallelism (slower)"
                );
                Psize::X8
            }
            _ => Psize::X32,
        }
    }

    pub(crate) fn granularity(self) -> u32 {
        match self {
            Psize::X8 => 1,
            Psize::X32 => 4,
        }
    }

    fn bits(self) -> u32 {
        let v = match self {
            Psize::X8 => 0b00,
            Psize::X32 => 0b10,
        };
        v << CR_PSIZE_SHIFT
    }
}

pub(crate) struct SectorFlash {
    geometry: FlashGeometry,
    psize: Psize,
}

impl SectorFlash {
    pub(crate) fn new(geometry: FlashGeometry, psize: Psize) -> Self {
        Self { geometry, psize }
    }

    /// SNB of a sector: its index in bank 1; `0x10 | index` in bank 2
    /// (sectors 12–23 in RM0090/RM0410 numbering).
    pub(crate) fn snb(sector: &Sector) -> u32 {
        if sector.bank == 0 {
            sector.index
        } else {
            0x10 | sector.index
        }
    }

    fn finish(m: &mut dyn Memory, timeout: Duration) -> Result<()> {
        let sr = ctrl::wait_idle(m, SR, SR_BSY, timeout)?;
        m.write_u32(SR, SR_EOP | SR_ERRORS)?;
        if sr & SR_ERRORS != 0 {
            debug!("flash error, SR 0x{sr:08X}");
        }
        ctrl::check_errors(sr, SR_ERRORS, SR_WRPERR)
    }
}

impl FlashDriver for SectorFlash {
    fn geometry(&self) -> &FlashGeometry {
        &self.geometry
    }

    /// One register set for both banks.
    fn unlock(&mut self, m: &mut dyn Memory, _bank: u8) -> Result<()> {
        unlock_with_keys(m, KEYR, CR, CR_LOCK)?;
        ctrl::wait_idle(m, SR, SR_BSY, PROGRAM_TIMEOUT)?;
        m.write_u32(SR, SR_EOP | SR_ERRORS)
    }

    fn lock(&mut self, m: &mut dyn Memory, _bank: u8) -> Result<()> {
        m.write_u32(CR, CR_LOCK)
    }

    fn erase_sector(&mut self, m: &mut dyn Memory, sector: &Sector) -> Result<()> {
        let cr = CR_SER | self.psize.bits() | (Self::snb(sector) << CR_SNB_SHIFT);
        m.write_u32(CR, cr)?;
        m.write_u32(CR, cr | CR_STRT)?;
        let r = Self::finish(m, SECTOR_ERASE_TIMEOUT);
        m.write_u32(CR, 0)?;
        r
    }

    fn erase_bank(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()> {
        let mer = if bank == 0 { CR_MER } else { CR_MER_BANK2 };
        let cr = mer | self.psize.bits();
        m.write_u32(CR, cr)?;
        m.write_u32(CR, cr | CR_STRT)?;
        let r = Self::finish(m, BANK_ERASE_TIMEOUT);
        m.write_u32(CR, 0)?;
        r
    }

    fn program(&mut self, m: &mut dyn Memory, addr: u32, data: &[u8]) -> Result<()> {
        self.geometry
            .bank_of(addr)
            .ok_or(Error::InvalidArgument("program address outside flash"))?;
        m.write_u32(CR, CR_PG | self.psize.bits())?;
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
            let written = match self.psize {
                Psize::X32 => m.write_block32(a, block),
                Psize::X8 => m.write_block8(a, block),
            };
            r = written.and_then(|()| Self::finish(m, PROGRAM_TIMEOUT));
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
