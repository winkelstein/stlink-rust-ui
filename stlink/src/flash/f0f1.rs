//! STM32F0/F1/F3 flash controller (FPEC; RM0091, RM0008, RM0316). Erase goes
//! through the registers over SWD; programming runs a small loader in SRAM
//! that writes halfwords and waits for BSY after each one. Halfwords streamed
//! over SWD faster than the flash programs them (~50–70 µs each) are dropped
//! without an error flag (seen on an F103 at 1.8 MHz SWD). F1 XL-density:
//! bank 2 has its own register set at `+0x40`. Option bytes are never touched.

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
/// One loader run: up to `MAX_BUFFER / 2` halfwords at ≤ 70 µs each.
const LOADER_TIMEOUT: Duration = Duration::from_secs(2);

/// SRAM layout while programming: loader code, stack top, data buffer.
const SRAM: u32 = 0x2000_0000;
const LOADER_ADDR: u32 = SRAM;
const BUFFER_ADDR: u32 = SRAM + 0x100;
/// Bytes per loader run (the smallest F0 has 4 KiB of SRAM).
const MAX_BUFFER: usize = 2 * 1024;

/// Thumb-1 (Cortex-M0 compatible) loader, assembled from:
///
/// ```text
/// @ r0 = source (RAM), r1 = destination (flash), r2 = halfword count (> 0),
/// @ r3 = FLASH_SR of the bank. PG is set by the host.
/// @ Exit (BKPT): r0 = halfwords left (0 = done), r1 = last SR.
/// start: ldrh r4, [r0]
///        strh r4, [r1]
/// wait:  ldr  r5, [r3]
///        movs r6, #1          @ BSY
///        tst  r5, r6
///        bne  wait
///        movs r6, #0x14       @ PGERR | WRPRTERR
///        tst  r5, r6
///        bne  done
///        adds r0, #2
///        adds r1, #2
///        subs r2, #1
///        bne  start
/// done:  movs r0, r2
///        movs r1, r5
///        bkpt #0
/// ```
pub(crate) const LOADER: [u8; 32] = [
    0x04, 0x88, 0x0c, 0x80, 0x1d, 0x68, 0x01, 0x26, 0x35, 0x42, 0xfb, 0xd1, 0x14, 0x26, 0x35, 0x42,
    0x03, 0xd1, 0x02, 0x30, 0x02, 0x31, 0x01, 0x3a, 0xf2, 0xd1, 0x10, 0x00, 0x29, 0x00, 0x00, 0xbe,
];

pub(crate) struct F0F1Flash {
    geometry: FlashGeometry,
    /// Bytes per loader run.
    buffer: usize,
}

impl F0F1Flash {
    /// `geometry`: uniform pages, one bank (two for F1 XL), halfword writes.
    /// `sram_size`: bytes of SRAM at 0x20000000 (for the loader buffer).
    pub(crate) fn new(geometry: FlashGeometry, sram_size: u32) -> Result<Self> {
        let buffer = (sram_size.saturating_sub(BUFFER_ADDR - SRAM) as usize).min(MAX_BUFFER) & !3;
        if buffer < 256 {
            return Err(Error::Unsupported(
                "flash programming without a known SRAM size",
            ));
        }
        Ok(Self { geometry, buffer })
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
        if addr % 2 != 0 || data.len() % 2 != 0 {
            return Err(Error::InvalidArgument("F0/F1/F3 program whole halfwords"));
        }
        m.write_block32(LOADER_ADDR, &LOADER)?;
        m.write_u32(Self::reg(bank, CR), CR_PG)?;
        let mut r = Ok(());
        let mut done = 0;
        while done < data.len() {
            let a = addr + done as u32;
            let n = self.buffer.min(data.len() - done);
            let block = &data[done..done + n];
            // Erased flash already reads as the erased value: nothing to program.
            if block.iter().all(|&b| b == self.geometry.erased_value) {
                done += n;
                continue;
            }
            r = self.run_loader(m, bank, a, block);
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

impl F0F1Flash {
    /// Copy `block` into the SRAM buffer and let the loader program it at `addr`.
    fn run_loader(&self, m: &mut dyn Memory, bank: u8, addr: u32, block: &[u8]) -> Result<()> {
        // 32-bit transfer: pad an odd halfword (the loader only programs `count`).
        let mut buf = block.to_vec();
        buf.resize(block.len().next_multiple_of(4), self.geometry.erased_value);
        m.write_block32(BUFFER_ADDR, &buf)?;
        let count = (block.len() / 2) as u32;
        let sr = Self::reg(bank, SR);
        let [left, status] = m.run_routine(
            LOADER_ADDR,
            [BUFFER_ADDR, addr, count, sr],
            BUFFER_ADDR,
            LOADER_TIMEOUT,
        )?;
        m.write_u32(sr, SR_EOP | SR_PGERR | SR_WRPRTERR)?;
        if left != 0 {
            debug!(
                "loader stopped at 0x{:08X}, SR 0x{status:08X}",
                addr + 2 * (count - left.min(count))
            );
            ctrl::check_errors(status, SR_PGERR, SR_WRPRTERR)?;
            return Err(Error::FlashOperation { status });
        }
        Ok(())
    }
}
