//! Register model of the flash controllers, for tests without hardware.
//!
//! Models KEYR/CR/SR/AR (CCR on H7), the lock and key sequence, BSY for a few
//! polls after each operation, EOP, error bits and the flash array. Wrong
//! sequences are recorded in `violations` (and set the error flag the real
//! controller would set): writing while locked, a wrong key order, flash
//! writes without PG, a wrong bus width or PSIZE, misaligned double words,
//! programming a word that is not blank, a bad SNB/PNB, starting an operation
//! while busy, any write to an option register. F0/F1/F3 programming runs
//! the SRAM loader (`f0f1::LOADER`), emulated here; streaming several
//! halfwords over the bus is a violation (the real controller drops them).

use std::collections::HashMap;
use std::time::Duration;

use super::f0f1::LOADER;
use super::geometry::FlashGeometry;
use super::mem::Memory;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    /// F0/F1/F3 FPEC (one register set; two with F1 XL).
    F0F1,
    /// F2/F4/F7: one register set at 0x40023C00.
    F4,
    G4,
    H7,
}

const KEY1: u32 = 0x4567_0123;
const KEY2: u32 = 0xCDEF_89AB;

/// Addresses of one register set.
#[derive(Debug, Clone, Copy)]
struct Regs {
    keyr: u32,
    sr: u32,
    cr: u32,
    ar: Option<u32>,
    /// H7: flags are cleared through CCR, not by writing SR.
    ccr: Option<u32>,
}

/// Bits that differ between families.
#[derive(Debug, Clone, Copy)]
struct Bits {
    lock: u32,
    pg: u32,
    strt: u32,
    bsy: u32,
    eop: u32,
    /// Sequence error (PGSERR / PGERR).
    seq: u32,
    /// Programming a non-blank word (PROGERR / PGERR).
    prog: u32,
    /// Alignment or width error.
    align: u32,
    /// Flags a write to SR clears (write 1 to clear).
    clearable: u32,
}

#[derive(Debug, Default, Clone)]
struct Bank {
    locked: bool,
    key_stage: u8,
    hard_locked: bool,
    cr: u32,
    sr: u32,
    ar: u32,
}

pub(crate) struct FakeFlash {
    family: Family,
    geometry: FlashGeometry,
    pub flash: Vec<u8>,
    banks: Vec<Bank>,
    /// Read-only registers (option bytes): address → value.
    pub opt: HashMap<u32, u32>,
    /// Wrong sequences seen by the model; empty after a correct flow.
    pub violations: Vec<String>,
    /// Instead of the next erase/program, set these SR bits.
    pub fail_next: Option<u32>,
    /// SR reads that still show BSY.
    busy_polls: u32,
    pub halted: bool,
    /// Start addresses of the erased units, in order (`u32::MAX` = bank `n` as `MAX - n`).
    pub erased: Vec<u32>,
    /// SRAM at `RAM_BASE`.
    ram: Vec<u8>,
    /// Routines run on the core.
    pub routines: u32,
}

const RAM_BASE: u32 = 0x2000_0000;
const RAM_SIZE: usize = 64 * 1024;

impl FakeFlash {
    /// A locked controller with the whole flash filled with `fill`.
    pub(crate) fn new(family: Family, geometry: FlashGeometry, fill: u8) -> Self {
        let sets = match family {
            Family::F0F1 | Family::H7 => geometry.banks.len().max(1),
            Family::F4 | Family::G4 => 1,
        };
        Self {
            family,
            flash: vec![fill; geometry.size as usize],
            geometry,
            banks: vec![
                Bank {
                    locked: true,
                    ..Bank::default()
                };
                sets
            ],
            opt: HashMap::new(),
            violations: Vec::new(),
            fail_next: None,
            busy_polls: 0,
            halted: false,
            erased: Vec::new(),
            ram: vec![0; RAM_SIZE],
            routines: 0,
        }
    }

    pub(crate) fn locked(&self) -> bool {
        self.banks.iter().all(|b| b.locked)
    }

    fn regs(&self, set: usize) -> Regs {
        let s = set as u32;
        match self.family {
            Family::F0F1 => {
                let b = 0x4002_2000 + s * 0x40;
                Regs {
                    keyr: b + 0x04,
                    sr: b + 0x0C,
                    cr: b + 0x10,
                    ar: Some(b + 0x14),
                    ccr: None,
                }
            }
            Family::F4 => {
                let b = 0x4002_3C00;
                Regs {
                    keyr: b + 0x04,
                    sr: b + 0x0C,
                    cr: b + 0x10,
                    ar: None,
                    ccr: None,
                }
            }
            Family::G4 => {
                let b = 0x4002_2000;
                Regs {
                    keyr: b + 0x08,
                    sr: b + 0x10,
                    cr: b + 0x14,
                    ar: None,
                    ccr: None,
                }
            }
            Family::H7 => {
                let b = 0x5200_2000 + s * 0x100;
                Regs {
                    keyr: b + 0x04,
                    cr: b + 0x0C,
                    sr: b + 0x10,
                    ar: None,
                    ccr: Some(b + 0x14),
                }
            }
        }
    }

    fn bits(&self) -> Bits {
        match self.family {
            Family::F0F1 => Bits {
                lock: 1 << 7,
                pg: 1 << 0,
                strt: 1 << 6,
                bsy: 1 << 0,
                eop: 1 << 5,
                seq: 1 << 2,
                prog: 1 << 2,
                align: 1 << 2,
                clearable: (1 << 5) | (1 << 4) | (1 << 2),
            },
            Family::F4 => Bits {
                lock: 1 << 31,
                pg: 1 << 0,
                strt: 1 << 16,
                bsy: 1 << 16,
                eop: 1 << 0,
                seq: 1 << 7,
                prog: 1 << 7,
                align: 1 << 6,
                clearable: 0xF3,
            },
            Family::G4 => Bits {
                lock: 1 << 31,
                pg: 1 << 0,
                strt: 1 << 16,
                bsy: 1 << 16,
                eop: 1 << 0,
                seq: 1 << 7,
                prog: 1 << 3,
                align: 1 << 5,
                clearable: 0x3FB,
            },
            Family::H7 => Bits {
                lock: 1 << 0,
                pg: 1 << 1,
                strt: 1 << 7,
                bsy: 1 << 0,
                eop: 1 << 16,
                seq: 1 << 18,
                prog: 1 << 18,
                align: 1 << 21,
                clearable: 0x0FEF_0000,
            },
        }
    }

    fn violation(&mut self, set: usize, what: String, flag: u32) {
        self.violations.push(what);
        if let Some(bank) = self.banks.get_mut(set) {
            bank.sr |= flag;
        }
    }

    fn in_flash(&self, addr: u32) -> bool {
        addr >= self.geometry.base && u64::from(addr) < self.geometry.end()
    }

    fn set_of_reg(&self, addr: u32) -> Option<(usize, Regs)> {
        (0..self.banks.len())
            .map(|i| (i, self.regs(i)))
            .find(|(_, r)| {
                [Some(r.keyr), Some(r.sr), Some(r.cr), r.ar, r.ccr].contains(&Some(addr))
            })
    }

    /// Register set that programs `addr`.
    fn set_of_flash(&self, addr: u32) -> usize {
        match self.family {
            Family::F0F1 | Family::H7 => {
                usize::from(self.geometry.bank_of(addr).unwrap_or(0)).min(self.banks.len() - 1)
            }
            Family::F4 | Family::G4 => 0,
        }
    }

    fn erase_range(&mut self, addr: u32, size: u32) {
        let off = (addr - self.geometry.base) as usize;
        self.flash[off..off + size as usize].fill(0xFF);
        self.erased.push(addr);
    }

    fn erase_bank(&mut self, bank: usize) {
        if let Some(b) = self.geometry.banks.get(bank).cloned() {
            let off = (b.base - self.geometry.base) as usize;
            self.flash[off..off + b.size() as usize].fill(0xFF);
            self.erased.push(u32::MAX - bank as u32);
        }
    }

    /// Unit `index` of `bank`, if it exists.
    fn unit(&self, bank: usize, index: u32) -> Option<(u32, u32)> {
        self.geometry
            .sectors()
            .into_iter()
            .find(|s| usize::from(s.bank) == bank && s.index == index)
            .map(|s| (s.addr, s.size))
    }

    /// CR written with STRT: run the selected operation.
    fn start(&mut self, set: usize, cr: u32) {
        let bits = self.bits();
        if self.busy_polls > 0 {
            self.violation(set, "operation started while BSY".into(), bits.seq);
            return;
        }
        if let Some(fail) = self.fail_next.take() {
            self.banks[set].sr |= fail;
            self.busy_polls = 2;
            return;
        }
        let dual = self.geometry.banks.len() > 1;
        let before = self.violations.len();
        let ok = match self.family {
            Family::F0F1 => {
                if cr & (1 << 1) != 0 {
                    let ar = self.banks[set].ar;
                    match self.geometry.sector_of(ar) {
                        Some(s) if usize::from(s.bank) == set => {
                            self.erase_range(s.addr, s.size);
                            true
                        }
                        _ => {
                            self.violation(
                                set,
                                format!("PER with AR 0x{ar:08X} outside bank {}", set + 1),
                                bits.seq,
                            );
                            false
                        }
                    }
                } else if cr & (1 << 2) != 0 {
                    self.erase_bank(set);
                    true
                } else {
                    false
                }
            }
            Family::F4 => {
                if cr & (1 << 1) != 0 {
                    let snb = (cr >> 3) & 0x1F;
                    let (bank, index) = if snb & 0x10 != 0 {
                        (1, snb & 0x0F)
                    } else {
                        (0, snb)
                    };
                    match self.unit(bank, index) {
                        Some((a, size)) if bank == 0 || dual => {
                            self.erase_range(a, size);
                            true
                        }
                        _ => {
                            self.violation(set, format!("bad SNB 0x{snb:02X}"), bits.seq);
                            false
                        }
                    }
                } else if cr & ((1 << 2) | (1 << 15)) != 0 {
                    if cr & (1 << 15) != 0 && !dual {
                        self.violation(set, "MER1/MER2 on a single-bank flash".into(), bits.seq);
                        false
                    } else {
                        if cr & (1 << 2) != 0 {
                            self.erase_bank(0);
                        }
                        if cr & (1 << 15) != 0 {
                            self.erase_bank(1);
                        }
                        true
                    }
                } else {
                    false
                }
            }
            Family::G4 => {
                if cr & (1 << 1) != 0 {
                    let pnb = (cr >> 3) & 0xFF;
                    let bker = (cr >> 11) & 1 == 1;
                    if bker && !dual {
                        self.violation(set, "BKER on a single-bank flash".into(), bits.seq);
                        false
                    } else {
                        match self.unit(usize::from(bker), pnb) {
                            Some((a, size)) => {
                                self.erase_range(a, size);
                                true
                            }
                            None => {
                                self.violation(
                                    set,
                                    format!("bad PNB {pnb} (bank {})", u8::from(bker) + 1),
                                    bits.seq,
                                );
                                false
                            }
                        }
                    }
                } else if cr & ((1 << 2) | (1 << 15)) != 0 {
                    if cr & (1 << 15) != 0 && !dual {
                        self.violation(set, "MER2 on a single-bank flash".into(), bits.seq);
                        false
                    } else {
                        if cr & (1 << 2) != 0 {
                            self.erase_bank(0);
                        }
                        if cr & (1 << 15) != 0 {
                            self.erase_bank(1);
                        }
                        true
                    }
                } else {
                    false
                }
            }
            Family::H7 => {
                if cr & (1 << 2) != 0 {
                    let snb = (cr >> 8) & 0x7;
                    match self.unit(set, snb) {
                        Some((a, size)) => {
                            self.erase_range(a, size);
                            true
                        }
                        None => {
                            self.violation(set, format!("bad SNB {snb}"), bits.seq);
                            false
                        }
                    }
                } else if cr & (1 << 3) != 0 {
                    self.erase_bank(set);
                    true
                } else {
                    false
                }
            }
        };
        if ok {
            self.banks[set].sr |= bits.eop;
        } else if self.violations.len() == before {
            self.violation(
                set,
                format!("STRT without an operation (CR 0x{cr:08X})"),
                bits.seq,
            );
        }
        self.busy_polls = 2;
    }

    fn write_reg(&mut self, set: usize, r: Regs, addr: u32, v: u32) {
        let bits = self.bits();
        if addr == r.keyr {
            let b = &mut self.banks[set];
            if b.hard_locked {
                self.violation(set, "key written after a wrong sequence".into(), bits.seq);
            } else if b.key_stage == 0 && v == KEY1 {
                b.key_stage = 1;
            } else if b.key_stage == 1 && v == KEY2 {
                b.key_stage = 0;
                b.locked = false;
            } else {
                b.hard_locked = true;
                self.violation(set, format!("wrong key sequence (0x{v:08X})"), bits.seq);
            }
        } else if Some(addr) == r.ccr || (addr == r.sr && r.ccr.is_none()) {
            self.banks[set].sr &= !(v & bits.clearable);
        } else if addr == r.sr {
            self.violation(
                set,
                "SR written on H7 (flags clear through CCR)".into(),
                bits.seq,
            );
        } else if Some(addr) == r.ar {
            if self.banks[set].locked {
                self.violation(set, "AR written while locked".into(), bits.seq);
            }
            self.banks[set].ar = v;
        } else if addr == r.cr {
            if v & bits.lock != 0 {
                self.banks[set].locked = true;
                self.banks[set].cr = 0;
            } else if self.banks[set].locked {
                self.violation(set, format!("CR 0x{v:08X} written while locked"), bits.seq);
            } else {
                self.banks[set].cr = v;
                if v & bits.strt != 0 {
                    self.start(set, v);
                }
            }
        }
    }

    /// A bus write of `width` bits into flash.
    fn program(&mut self, addr: u32, data: &[u8], width: u32) {
        let bits = self.bits();
        let set = self.set_of_flash(addr);
        let cr = self.banks[set].cr;
        if self.banks[set].locked || cr & bits.pg == 0 {
            self.violation(
                set,
                format!("flash write at 0x{addr:08X} without PG"),
                bits.seq,
            );
            return;
        }
        if self.busy_polls > 0 {
            self.violation(set, "flash write while BSY".into(), bits.seq);
            return;
        }
        let ok_width = match self.family {
            // One halfword per write: the next one would arrive while BSY.
            Family::F0F1 => width == 16 && data.len() == 2,
            Family::F4 => {
                let psize = match (cr >> 8) & 0b11 {
                    0 => 8,
                    1 => 16,
                    2 => 32,
                    _ => 64,
                };
                width == psize
            }
            Family::G4 => width == 32 && addr % 8 == 0 && data.len() % 8 == 0,
            Family::H7 => width == 32 && addr % 32 == 0 && data.len() % 32 == 0,
        };
        if !ok_width {
            self.violation(
                set,
                format!(
                    "{width}-bit write of {} bytes at 0x{addr:08X} (CR 0x{cr:08X})",
                    data.len()
                ),
                bits.align,
            );
            return;
        }
        if let Some(fail) = self.fail_next.take() {
            self.banks[set].sr |= fail;
            self.busy_polls = 1;
            return;
        }
        let off = (addr - self.geometry.base) as usize;
        if self.flash[off..off + data.len()].iter().any(|&b| b != 0xFF) {
            self.violation(
                set,
                format!("programming non-blank flash at 0x{addr:08X}"),
                bits.prog,
            );
            return;
        }
        self.flash[off..off + data.len()].copy_from_slice(data);
        self.banks[set].sr |= bits.eop;
        self.busy_polls = 1;
    }

    fn ram_range(&self, addr: u32, len: usize) -> Option<std::ops::Range<usize>> {
        let off = addr.checked_sub(RAM_BASE)? as usize;
        (off + len <= self.ram.len()).then_some(off..off + len)
    }

    fn write(&mut self, addr: u32, data: &[u8], width: u32) -> Result<()> {
        if let Some(r) = self.ram_range(addr, data.len()) {
            self.ram[r].copy_from_slice(data);
            return Ok(());
        }
        if self.in_flash(addr) {
            if !self.in_flash(addr + data.len() as u32 - 1) {
                return Err(Error::InvalidArgument("fake: write past the end of flash"));
            }
            self.program(addr, data, width);
            return Ok(());
        }
        if self.opt.contains_key(&addr) {
            self.violations
                .push(format!("option register 0x{addr:08X} written"));
            return Ok(());
        }
        if let Some((set, r)) = self.set_of_reg(addr) {
            for (i, word) in data.chunks(4).enumerate() {
                let mut w = [0u8; 4];
                w[..word.len()].copy_from_slice(word);
                self.write_reg(set, r, addr + 4 * i as u32, u32::from_le_bytes(w));
            }
        }
        Ok(())
    }
}

impl Memory for FakeFlash {
    fn read_u32(&mut self, addr: u32) -> Result<u32> {
        if let Some((set, r)) = self.set_of_reg(addr) {
            let bits = self.bits();
            let b = &self.banks[set];
            return Ok(if addr == r.sr {
                let sr = b.sr;
                if self.busy_polls > 0 {
                    self.busy_polls -= 1;
                    sr | bits.bsy
                } else {
                    sr
                }
            } else if addr == r.cr {
                b.cr | if b.locked { bits.lock } else { 0 }
            } else if Some(addr) == r.ar {
                b.ar
            } else {
                0
            });
        }
        if let Some(v) = self.opt.get(&addr) {
            return Ok(*v);
        }
        let bytes = self.read_block(addr, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn write_u32(&mut self, addr: u32, value: u32) -> Result<()> {
        self.write(addr, &value.to_le_bytes(), 32)
    }

    fn write_block32(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        if addr % 4 != 0 || data.len() % 4 != 0 {
            return Err(Error::InvalidArgument("fake: unaligned 32-bit write"));
        }
        self.write(addr, data, 32)
    }

    fn write_block8(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        self.write(addr, data, 8)
    }

    fn read_block(&mut self, addr: u32, len: usize) -> Result<Vec<u8>> {
        if let Some(r) = self.ram_range(addr, len) {
            return Ok(self.ram[r].to_vec());
        }
        Ok((0..len as u32)
            .map(|i| {
                let a = addr.wrapping_add(i);
                if self.in_flash(a) {
                    self.flash[(a - self.geometry.base) as usize]
                } else {
                    0
                }
            })
            .collect())
    }

    fn halt(&mut self) -> Result<()> {
        self.halted = true;
        Ok(())
    }

    /// Emulates `f0f1::LOADER`: one halfword at a time, BSY waited out,
    /// stop on PGERR/WRPRTERR.
    fn run_routine(
        &mut self,
        entry: u32,
        args: [u32; 4],
        _sp: u32,
        _timeout: Duration,
    ) -> Result<[u32; 2]> {
        if !self.halted {
            return Err(Error::InvalidArgument(
                "fake: routine run on a running core",
            ));
        }
        let code = self.ram_range(entry, LOADER.len()).map(|r| &self.ram[r]);
        if self.family != Family::F0F1 || code != Some(&LOADER[..]) {
            return Err(Error::InvalidArgument("fake: unknown routine"));
        }
        self.routines += 1;
        let [src, dst, count, sr_addr] = args;
        let set = self
            .set_of_reg(sr_addr)
            .filter(|(_, r)| r.sr == sr_addr)
            .map(|(set, _)| set)
            .ok_or(Error::InvalidArgument("fake: loader without an SR address"))?;
        let mut left = count;
        let mut sr = 0;
        while left > 0 {
            let i = count - left;
            let half = self
                .ram_range(src + 2 * i, 2)
                .map(|r| self.ram[r].to_vec())
                .ok_or(Error::InvalidArgument("fake: loader source outside SRAM"))?;
            self.write(dst + 2 * i, &half, 16)?;
            self.busy_polls = 0;
            sr = self.banks[set].sr;
            if sr & ((1 << 2) | (1 << 4)) != 0 {
                break;
            }
            left -= 1;
        }
        Ok([left, sr])
    }

    fn delay(&mut self, _d: Duration) {}
}
