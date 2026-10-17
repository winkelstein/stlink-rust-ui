//! STM32H74x/H75x flash controller (RM0433 §4), driven by direct register
//! access over SWD. Two banks, 128 KiB sectors, 256-bit (32-byte) flash words.
//! Option bytes are never touched.

use std::thread;
use std::time::{Duration, Instant};

use tracing::debug;

use super::FlashDriver;
use super::geometry::{FlashGeometry, Sector};
use crate::probe::StLink;
use crate::{Error, Result};

const BANK1_REGS: u32 = 0x5200_2000;
const BANK2_OFFSET: u32 = 0x100;

const KEYR: u32 = 0x04;
const CR: u32 = 0x0C;
const SR: u32 = 0x10;
const CCR: u32 = 0x14;

const KEY1: u32 = 0x4567_0123;
const KEY2: u32 = 0xCDEF_89AB;

const CR_LOCK: u32 = 1 << 0;
const CR_PG: u32 = 1 << 1;
const CR_SER: u32 = 1 << 2;
const CR_BER: u32 = 1 << 3;
/// PSIZE = x32 parallelism.
const CR_PSIZE_X32: u32 = 0b10 << 4;
const CR_START: u32 = 1 << 7;
const CR_SNB_SHIFT: u32 = 8;

const SR_BSY: u32 = 1 << 0;
const SR_WBNE: u32 = 1 << 1;
const SR_QW: u32 = 1 << 2;
const SR_WRPERR: u32 = 1 << 17;
const SR_RDPERR: u32 = 1 << 23;
/// WRPERR, PGSERR, STRBERR, INCERR, OPERR, RDPERR, RDSERR, SNECCERR, DBECCERR.
const SR_ERRORS: u32 = 0x07EE_0000;
/// All CLR_* bits of FLASH_CCR (EOP, errors, CRCEND).
const CCR_ALL: u32 = 0x0FEF_0000;

const PROGRAM_TIMEOUT: Duration = Duration::from_millis(100);
/// Bytes per write transfer: 32 flash words, the ADIv5 TAR auto-increment
/// window. Measured on H743 + V2 clone @4 MHz: 32 B → 38 KiB/s, 1 KiB → 135 KiB/s.
/// Not retried on WAIT: re-sending would program the same flash words twice.
const PROGRAM_CHUNK: usize = 1024;
/// RM0433: sector erase is ~2 s max at x32; leave margin.
const ERASE_TIMEOUT: Duration = Duration::from_secs(10);
/// RM0433: bank erase is ~16 s max at x32 (8 sectors); leave margin.
const BANK_ERASE_TIMEOUT: Duration = Duration::from_secs(40);

pub(crate) struct H7Flash {
    geometry: FlashGeometry,
}

impl H7Flash {
    pub(crate) fn new(flash_size: u32, sector_size: u32, dual_bank: bool) -> Self {
        Self {
            geometry: FlashGeometry {
                base: crate::chips::FLASH_BASE,
                size: flash_size,
                bank_size: if dual_bank {
                    flash_size / 2
                } else {
                    flash_size
                },
                sector_size,
                write_granularity: 32,
                erased_value: 0xFF,
            },
        }
    }

    fn reg(bank: u8, offset: u32) -> u32 {
        BANK1_REGS + u32::from(bank) * BANK2_OFFSET + offset
    }

    /// Wait until the bank is idle, then check and clear its error flags.
    fn wait_idle(link: &mut StLink, bank: u8, busy: u32, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        let mut sr;
        loop {
            sr = link.read_u32(Self::reg(bank, SR))?;
            if sr & busy == 0 {
                break;
            }
            if start.elapsed() > timeout {
                debug!("flash bank {} busy, SR 0x{sr:08X}", bank + 1);
                return Err(Error::Timeout);
            }
            if timeout >= Duration::from_secs(1) {
                thread::sleep(Duration::from_millis(5));
            }
        }
        if sr & SR_ERRORS != 0 {
            link.write_u32(Self::reg(bank, CCR), CCR_ALL)?;
            debug!("flash bank {} error, SR 0x{sr:08X}", bank + 1);
            return Err(if sr & (SR_WRPERR | SR_RDPERR) != 0 {
                Error::FlashProtected
            } else {
                Error::FlashOperation { status: sr }
            });
        }
        Ok(())
    }
}

impl FlashDriver for H7Flash {
    fn geometry(&self) -> FlashGeometry {
        self.geometry
    }

    fn unlock(&mut self, link: &mut StLink, bank: u8) -> Result<()> {
        let cr = link.read_u32(Self::reg(bank, CR))?;
        if cr & CR_LOCK != 0 {
            link.write_u32(Self::reg(bank, KEYR), KEY1)?;
            link.write_u32(Self::reg(bank, KEYR), KEY2)?;
            if link.read_u32(Self::reg(bank, CR))? & CR_LOCK != 0 {
                return Err(Error::FlashProtected);
            }
        }
        Self::wait_idle(link, bank, SR_BSY | SR_QW | SR_WBNE, PROGRAM_TIMEOUT)?;
        link.write_u32(Self::reg(bank, CCR), CCR_ALL)
    }

    fn lock(&mut self, link: &mut StLink, bank: u8) -> Result<()> {
        link.write_u32(Self::reg(bank, CR), CR_LOCK)
    }

    fn erase_sector(&mut self, link: &mut StLink, sector: &Sector) -> Result<()> {
        let bank = sector.bank;
        let cr = CR_SER | CR_PSIZE_X32 | ((sector.index & 0x7) << CR_SNB_SHIFT);
        link.write_u32(Self::reg(bank, CR), cr)?;
        link.write_u32(Self::reg(bank, CR), cr | CR_START)?;
        let r = Self::wait_idle(link, bank, SR_BSY | SR_QW, ERASE_TIMEOUT);
        link.write_u32(Self::reg(bank, CR), 0)?;
        r
    }

    fn erase_bank(&mut self, link: &mut StLink, bank: u8) -> Result<()> {
        // BER erases the user flash of one bank; option bytes stay untouched.
        let cr = CR_BER | CR_PSIZE_X32;
        link.write_u32(Self::reg(bank, CR), cr)?;
        link.write_u32(Self::reg(bank, CR), cr | CR_START)?;
        let r = Self::wait_idle(link, bank, SR_BSY | SR_QW, BANK_ERASE_TIMEOUT);
        link.write_u32(Self::reg(bank, CR), 0)?;
        r
    }

    fn program(&mut self, link: &mut StLink, addr: u32, data: &[u8]) -> Result<()> {
        let bank = self.geometry.bank_of(addr);
        link.write_u32(Self::reg(bank, CR), CR_PG | CR_PSIZE_X32)?;
        let mut r = Ok(());
        let mut done = 0;
        while done < data.len() {
            let a = addr + done as u32;
            // Several flash words per transfer, never crossing a chunk-aligned
            // (≤ 1 KiB) boundary; the AXI bus stalls while the flash is busy.
            let n = (PROGRAM_CHUNK - (a as usize % PROGRAM_CHUNK)).min(data.len() - done);
            let block = &data[done..done + n];
            // Erased flash already reads as the erased value: nothing to program.
            if block.iter().all(|&b| b == self.geometry.erased_value) {
                done += n;
                continue;
            }
            r = link.write_mem32(a, &data[done..done + n]).and_then(|()| {
                Self::wait_idle(link, bank, SR_QW | SR_BSY | SR_WBNE, PROGRAM_TIMEOUT)
            });
            if r.is_err() {
                debug!("programming failed in 0x{a:08X}..0x{:08X}", a as usize + n);
                break;
            }
            done += n;
        }
        link.write_u32(Self::reg(bank, CR), 0)?;
        r
    }
}
