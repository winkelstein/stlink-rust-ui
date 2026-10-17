//! STM32F0/F1/F3 flash controller (FPEC; RM0091, RM0008, RM0316), driven by
//! direct register access over SWD. Pages of `flash_pagesize`, programmed in
//! halfwords (16-bit bus writes). F1 XL-density: bank 2 has its own register
//! set at `+0x40`. Option bytes are never touched.

use std::time::Duration;

use tracing::debug;

use super::FlashDriver;
use super::ctrl::{self, unlock_with_keys};
use super::geometry::{FlashGeometry, Sector};
use super::mem::Memory;
use crate::{Error, Result};

const FPEC: u32 = 0x4002_2000;
/// F1 XL: second register set (KEYR2, SR2, CR2, AR2).
const BANK2_OFFSET: u32 = 0x40;

const KEYR: u32 = 0x04;
const SR: u32 = 0x0C;
const CR: u32 = 0x10;
const AR: u32 = 0x14;

const SR_BSY: u32 = 1 << 0;
const SR_PGERR: u32 = 1 << 2;
const SR_WRPRTERR: u32 = 1 << 4;
const SR_EOP: u32 = 1 << 5;

const CR_PG: u32 = 1 << 0;
const CR_PER: u32 = 1 << 1;
const CR_MER: u32 = 1 << 2;
const CR_STRT: u32 = 1 << 6;
const CR_LOCK: u32 = 1 << 7;

/// Datasheets: page erase ≤ 40 ms, mass erase ≤ 40 ms (F0/F1); halfword
/// program ≤ 70 µs. Generous margins: the probe adds USB latency.
const PAGE_ERASE_TIMEOUT: Duration = Duration::from_secs(1);
const MASS_ERASE_TIMEOUT: Duration = Duration::from_secs(5);
const PROGRAM_TIMEOUT: Duration = Duration::from_millis(200);
/// Halfwords per bus transfer: the bus stalls while each one is programmed.
const PROGRAM_CHUNK: usize = 1024;

pub(crate) struct F0F1Flash {
    geometry: FlashGeometry,
}

impl F0F1Flash {
    /// `geometry`: uniform pages, one bank (two for F1 XL), halfword writes.
    pub(crate) fn new(geometry: FlashGeometry) -> Self {
        Self { geometry }
    }

    fn reg(bank: u8, offset: u32) -> u32 {
        FPEC + u32::from(bank) * BANK2_OFFSET + offset
    }

    /// Wait for !BSY, then map and clear the error flags (write 1 to clear).
    fn finish(m: &mut dyn Memory, bank: u8, timeout: Duration) -> Result<()> {
        let sr = ctrl::wait_idle(m, Self::reg(bank, SR), SR_BSY, timeout)?;
        m.write_u32(Self::reg(bank, SR), SR_EOP | SR_PGERR | SR_WRPRTERR)?;
        if sr & (SR_PGERR | SR_WRPRTERR) != 0 {
            debug!("flash bank {} error, SR 0x{sr:08X}", bank + 1);
        }
        ctrl::check_errors(sr, SR_PGERR, SR_WRPRTERR)
    }
}

impl FlashDriver for F0F1Flash {
    fn geometry(&self) -> &FlashGeometry {
        &self.geometry
    }

    fn unlock(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()> {
        unlock_with_keys(m, Self::reg(bank, KEYR), Self::reg(bank, CR), CR_LOCK)?;
        ctrl::wait_idle(m, Self::reg(bank, SR), SR_BSY, PROGRAM_TIMEOUT)?;
        m.write_u32(Self::reg(bank, SR), SR_EOP | SR_PGERR | SR_WRPRTERR)
    }

    fn lock(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()> {
        m.write_u32(Self::reg(bank, CR), CR_LOCK)
    }

    fn erase_sector(&mut self, m: &mut dyn Memory, page: &Sector) -> Result<()> {
        let bank = page.bank;
        m.write_u32(Self::reg(bank, CR), CR_PER)?;
        m.write_u32(Self::reg(bank, AR), page.addr)?;
        m.write_u32(Self::reg(bank, CR), CR_PER | CR_STRT)?;
        let r = Self::finish(m, bank, PAGE_ERASE_TIMEOUT);
        m.write_u32(Self::reg(bank, CR), 0)?;
        r
    }

    fn erase_bank(&mut self, m: &mut dyn Memory, bank: u8) -> Result<()> {
        // MER erases the main flash of this register set only; option bytes stay.
        m.write_u32(Self::reg(bank, CR), CR_MER)?;
        m.write_u32(Self::reg(bank, CR), CR_MER | CR_STRT)?;
        let r = Self::finish(m, bank, MASS_ERASE_TIMEOUT);
        m.write_u32(Self::reg(bank, CR), 0)?;
        r
    }

    fn program(&mut self, m: &mut dyn Memory, addr: u32, data: &[u8]) -> Result<()> {
        let bank = self
            .geometry
            .bank_of(addr)
            .ok_or(Error::InvalidArgument("program address outside flash"))?;
        m.write_u32(Self::reg(bank, CR), CR_PG)?;
        let mut r = Ok(());
        let mut done = 0;
        while done < data.len() {
            let a = addr + done as u32;
            let n = (PROGRAM_CHUNK - (a as usize % PROGRAM_CHUNK)).min(data.len() - done);
            let block = &data[done..done + n];
            // Erased flash already reads as the erased value: nothing to program.
            if block.iter().all(|&b| b == self.geometry.erased_value) {
                done += n;
                continue;
            }
            r = m
                .write_block16(a, block)
                .and_then(|()| Self::finish(m, bank, PROGRAM_TIMEOUT));
            if r.is_err() {
                debug!("programming failed in 0x{a:08X}..0x{:08X}", a as usize + n);
                break;
            }
            done += n;
        }
        m.write_u32(Self::reg(bank, CR), 0)?;
        r
    }
}
