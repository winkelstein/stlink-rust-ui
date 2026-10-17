//! Flash layout and write planning (pure logic, no device access).

use crate::{Error, Result};

/// Uniform-sector flash layout, possibly split into equal banks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashGeometry {
    pub base: u32,
    pub size: u32,
    pub bank_size: u32,
    pub sector_size: u32,
    /// Programming unit; data is padded to it with `erased_value`.
    pub write_granularity: u32,
    pub erased_value: u8,
}

/// One erasable sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sector {
    pub bank: u8,
    /// Index within the bank.
    pub index: u32,
    pub addr: u32,
}

/// What a write of a given range has to touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritePlan {
    pub addr: u32,
    /// Length after padding to the write granularity.
    pub padded_len: u32,
    pub sectors: Vec<Sector>,
}

impl FlashGeometry {
    pub fn end(&self) -> u64 {
        u64::from(self.base) + u64::from(self.size)
    }

    /// Bank number of an address inside flash.
    pub fn bank_of(&self, addr: u32) -> u8 {
        ((addr - self.base) / self.bank_size.max(1)) as u8
    }

    fn sector_of(&self, addr: u32) -> Sector {
        let bank = self.bank_of(addr);
        let bank_base = self.base + u32::from(bank) * self.bank_size;
        let index = (addr - bank_base) / self.sector_size;
        Sector {
            bank,
            index,
            addr: bank_base + index * self.sector_size,
        }
    }

    /// Validate a write of `len` bytes at `addr` and list the sectors to erase.
    /// Everything is checked here, before anything destructive happens.
    pub fn plan_write(&self, addr: u32, len: usize) -> Result<WritePlan> {
        if len == 0 {
            return Err(Error::InvalidArgument("nothing to write"));
        }
        if self.sector_size == 0 || self.write_granularity == 0 || self.bank_size == 0 {
            return Err(Error::InvalidArgument("invalid flash geometry"));
        }
        if addr < self.base || u64::from(addr) >= self.end() {
            return Err(Error::InvalidArgument("address outside flash"));
        }
        if addr % self.write_granularity != 0 {
            return Err(Error::InvalidArgument(
                "address not aligned to the flash write granularity",
            ));
        }
        let padded = (len as u64).div_ceil(u64::from(self.write_granularity))
            * u64::from(self.write_granularity);
        if u64::from(addr) + padded > self.end() {
            return Err(Error::InvalidArgument("data does not fit into flash"));
        }
        let padded_len = padded as u32;
        let last = addr + (padded_len - 1);
        let mut sectors = Vec::new();
        let mut a = self.sector_of(addr).addr;
        while a <= last {
            sectors.push(self.sector_of(a));
            match a.checked_add(self.sector_size) {
                Some(next) => a = next,
                None => break,
            }
        }
        Ok(WritePlan {
            addr,
            padded_len,
            sectors,
        })
    }
}

/// `data` padded with `value` to `len` bytes.
pub fn pad(data: &[u8], len: usize, value: u8) -> Vec<u8> {
    let mut v = data.to_vec();
    v.resize(len.max(data.len()), value);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h743() -> FlashGeometry {
        FlashGeometry {
            base: 0x0800_0000,
            size: 2 * 1024 * 1024,
            bank_size: 1024 * 1024,
            sector_size: 128 * 1024,
            write_granularity: 32,
            erased_value: 0xFF,
        }
    }

    #[test]
    fn small_image_one_sector() {
        let p = h743().plan_write(0x0800_0000, 16892);
        let p = p.as_ref().ok();
        assert_eq!(p.map(|p| p.padded_len), Some(16896));
        assert_eq!(
            p.map(|p| p.sectors.clone()),
            Some(vec![Sector {
                bank: 0,
                index: 0,
                addr: 0x0800_0000
            }])
        );
    }

    #[test]
    fn spans_banks() {
        let p = h743().plan_write(0x080E_0000, 256 * 1024);
        let sectors = p.map(|p| p.sectors).unwrap_or_default();
        assert_eq!(
            sectors,
            vec![
                Sector {
                    bank: 0,
                    index: 7,
                    addr: 0x080E_0000
                },
                Sector {
                    bank: 1,
                    index: 0,
                    addr: 0x0810_0000
                },
            ]
        );
    }

    #[test]
    fn full_flash() {
        let p = h743().plan_write(0x0800_0000, 2 * 1024 * 1024);
        assert_eq!(p.map(|p| p.sectors.len()).ok(), Some(16));
    }

    #[test]
    fn rejects_bad_input() {
        let g = h743();
        assert!(g.plan_write(0x0800_0010, 32).is_err(), "unaligned");
        assert!(g.plan_write(0x0700_0000, 32).is_err(), "below flash");
        assert!(g.plan_write(0x0820_0000, 32).is_err(), "above flash");
        assert!(g.plan_write(0x081F_FFE0, 64).is_err(), "too big");
        assert!(g.plan_write(0x0800_0000, 0).is_err(), "empty");
        assert!(
            g.plan_write(0x0800_0000, 2 * 1024 * 1024 + 1).is_err(),
            "past end"
        );
    }

    #[test]
    fn padding() {
        assert_eq!(pad(&[1, 2], 4, 0xFF), vec![1, 2, 0xFF, 0xFF]);
        assert_eq!(pad(&[1, 2], 2, 0xFF), vec![1, 2]);
    }
}
