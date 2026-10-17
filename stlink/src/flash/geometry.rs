//! Flash layout and write planning (pure logic, no device access).
//!
//! A flash is a list of banks; each bank is a list of runs of equally sized
//! erase units (pages on F0/F1/G4, sectors on F2/F4/F7/H7). Sizes may differ
//! between runs (F4: 16, 64 and 128 KiB sectors).

use crate::{Error, Result};

/// What the erase unit is called (pages vs sectors) — for UI texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseUnit {
    Page,
    Sector,
}

/// `count` erase units of `size` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectorRun {
    pub count: u32,
    pub size: u32,
}

/// One bank: contiguous erase units starting at `base`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashBank {
    pub base: u32,
    pub runs: Vec<SectorRun>,
}

impl FlashBank {
    pub fn size(&self) -> u32 {
        self.runs.iter().map(|r| r.count * r.size).sum()
    }

    pub fn sector_count(&self) -> u32 {
        self.runs.iter().map(|r| r.count).sum()
    }
}

/// Flash layout of a chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashGeometry {
    pub base: u32,
    pub size: u32,
    pub banks: Vec<FlashBank>,
    pub unit: EraseUnit,
    /// Programming unit; data is padded to it with `erased_value`.
    pub write_granularity: u32,
    pub erased_value: u8,
}

/// One erasable page or sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sector {
    pub bank: u8,
    /// Index within the bank.
    pub index: u32,
    pub addr: u32,
    pub size: u32,
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
    /// Banks laid out back to back from `base`; `size` is their sum.
    pub fn new(
        base: u32,
        banks: Vec<Vec<SectorRun>>,
        unit: EraseUnit,
        write_granularity: u32,
    ) -> Self {
        let mut addr = base;
        let banks: Vec<FlashBank> = banks
            .into_iter()
            .map(|runs| {
                let bank = FlashBank { base: addr, runs };
                addr = addr.wrapping_add(bank.size());
                bank
            })
            .collect();
        Self {
            base,
            size: addr.wrapping_sub(base),
            banks,
            unit,
            write_granularity,
            erased_value: 0xFF,
        }
    }

    /// `banks` equal banks of uniform units.
    pub fn uniform(
        base: u32,
        size: u32,
        banks: u32,
        unit_size: u32,
        unit: EraseUnit,
        write_granularity: u32,
    ) -> Self {
        let banks = banks.max(1);
        let count = size / banks / unit_size.max(1);
        Self::new(
            base,
            (0..banks)
                .map(|_| {
                    vec![SectorRun {
                        count,
                        size: unit_size,
                    }]
                })
                .collect(),
            unit,
            write_granularity,
        )
    }

    /// `pages 64 × 1 KiB`, `sectors 4 × 16 KiB, 1 × 64 KiB, 7 × 128 KiB | …`
    /// (banks separated by `|`).
    pub fn describe(&self) -> String {
        let unit = match self.unit {
            EraseUnit::Page => "pages",
            EraseUnit::Sector => "sectors",
        };
        let banks: Vec<String> = self
            .banks
            .iter()
            .map(|b| {
                b.runs
                    .iter()
                    .map(|r| format!("{} × {} KiB", r.count, r.size / 1024))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect();
        format!("{unit} {}", banks.join(" | "))
    }

    pub fn end(&self) -> u64 {
        u64::from(self.base) + u64::from(self.size)
    }

    pub fn bank_count(&self) -> u8 {
        self.banks.len().min(usize::from(u8::MAX)) as u8
    }

    /// End (exclusive) of bank `bank`.
    pub fn bank_end(&self, bank: u8) -> u64 {
        self.banks
            .get(usize::from(bank))
            .map_or(self.end(), |b| u64::from(b.base) + u64::from(b.size()))
    }

    /// Bank number of an address inside flash.
    pub fn bank_of(&self, addr: u32) -> Option<u8> {
        self.banks
            .iter()
            .position(|b| {
                addr >= b.base && u64::from(addr) < u64::from(b.base) + u64::from(b.size())
            })
            .map(|i| i as u8)
    }

    /// All erase units in address order (at most a few hundred).
    pub fn sectors(&self) -> Vec<Sector> {
        let mut out = Vec::new();
        for (b, bank) in self.banks.iter().enumerate() {
            let mut addr = bank.base;
            let mut index = 0;
            for run in &bank.runs {
                for _ in 0..run.count {
                    out.push(Sector {
                        bank: b as u8,
                        index,
                        addr,
                        size: run.size,
                    });
                    addr = addr.wrapping_add(run.size);
                    index += 1;
                }
            }
        }
        out
    }

    /// The erase unit containing `addr`.
    pub fn sector_of(&self, addr: u32) -> Option<Sector> {
        self.sectors()
            .into_iter()
            .find(|s| addr >= s.addr && u64::from(addr) < u64::from(s.addr) + u64::from(s.size))
    }

    /// Validate a write of `len` bytes at `addr` and list the units to erase.
    /// Everything is checked here, before anything destructive happens.
    pub fn plan_write(&self, addr: u32, len: usize) -> Result<WritePlan> {
        if len == 0 {
            return Err(Error::InvalidArgument("nothing to write"));
        }
        if self.banks.is_empty() || self.size == 0 || self.write_granularity == 0 {
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
        let end = u64::from(addr) + padded;
        let sectors = self
            .sectors()
            .into_iter()
            .filter(|s| {
                u64::from(s.addr) < end && u64::from(s.addr) + u64::from(s.size) > u64::from(addr)
            })
            .collect();
        Ok(WritePlan {
            addr,
            padded_len: padded as u32,
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

/// A bank layout template (from `chips/layouts.txt`) cut to `size` bytes.
/// `None` if the template is smaller than `size`.
pub(crate) fn truncate_runs(runs: &[SectorRun], size: u32) -> Option<Vec<SectorRun>> {
    let mut out = Vec::new();
    let mut left = size;
    for run in runs {
        if left == 0 {
            break;
        }
        let count = (left / run.size).min(run.count);
        if count == 0 {
            break;
        }
        out.push(SectorRun {
            count,
            size: run.size,
        });
        left -= count * run.size;
    }
    (left == 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIB: u32 = 1024;

    fn h743() -> FlashGeometry {
        FlashGeometry::uniform(0x0800_0000, 2048 * KIB, 2, 128 * KIB, EraseUnit::Sector, 32)
    }

    /// F42x 2 MiB, dual bank: 4 × 16, 64, 7 × 128 KiB per bank.
    fn f427() -> FlashGeometry {
        let bank = vec![
            SectorRun {
                count: 4,
                size: 16 * KIB,
            },
            SectorRun {
                count: 1,
                size: 64 * KIB,
            },
            SectorRun {
                count: 7,
                size: 128 * KIB,
            },
        ];
        FlashGeometry::new(0x0800_0000, vec![bank.clone(), bank], EraseUnit::Sector, 4)
    }

    fn sector(bank: u8, index: u32, addr: u32, size: u32) -> Sector {
        Sector {
            bank,
            index,
            addr,
            size,
        }
    }

    #[test]
    fn small_image_one_sector() {
        let p = h743().plan_write(0x0800_0000, 16892);
        let p = p.as_ref().ok();
        assert_eq!(p.map(|p| p.padded_len), Some(16896));
        assert_eq!(
            p.map(|p| p.sectors.clone()),
            Some(vec![sector(0, 0, 0x0800_0000, 128 * KIB)])
        );
    }

    #[test]
    fn spans_banks() {
        let p = h743().plan_write(0x080E_0000, 256 * 1024);
        let sectors = p.map(|p| p.sectors).unwrap_or_default();
        assert_eq!(
            sectors,
            vec![
                sector(0, 7, 0x080E_0000, 128 * KIB),
                sector(1, 0, 0x0810_0000, 128 * KIB),
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
    fn non_uniform_sectors() {
        let g = f427();
        assert_eq!(g.size, 2048 * KIB);
        assert_eq!(g.bank_count(), 2);
        assert_eq!(g.sectors().len(), 24);
        // 70 KiB from the start: sectors 0-3 (64 KiB) and sector 4 (64 KiB).
        let p = g
            .plan_write(0x0800_0000, 70 * 1024)
            .map(|p| p.sectors)
            .unwrap_or_default();
        assert_eq!(p.len(), 5);
        assert_eq!(p[4], sector(0, 4, 0x0801_0000, 64 * KIB));
        // Bank 2 starts at 1 MiB with a 16 KiB sector.
        assert_eq!(
            g.sector_of(0x0810_0000),
            Some(sector(1, 0, 0x0810_0000, 16 * KIB))
        );
        assert_eq!(g.bank_of(0x080F_FFFF), Some(0));
        assert_eq!(g.bank_of(0x0820_0000), None);
        assert_eq!(g.bank_end(0), 0x0810_0000);
    }

    #[test]
    fn writes_across_the_bank_boundary_of_a_non_uniform_flash() {
        let g = f427();
        let p = g
            .plan_write(0x080E_0000, 256 * 1024)
            .map(|p| p.sectors)
            .unwrap_or_default();
        assert_eq!(
            p,
            vec![
                sector(0, 11, 0x080E_0000, 128 * KIB),
                sector(1, 0, 0x0810_0000, 16 * KIB),
                sector(1, 1, 0x0810_4000, 16 * KIB),
                sector(1, 2, 0x0810_8000, 16 * KIB),
                sector(1, 3, 0x0810_C000, 16 * KIB),
                sector(1, 4, 0x0811_0000, 64 * KIB),
            ]
        );
    }

    #[test]
    fn templates_are_cut_to_the_flash_size() {
        let f4 = [
            SectorRun {
                count: 4,
                size: 16 * KIB,
            },
            SectorRun {
                count: 1,
                size: 64 * KIB,
            },
            SectorRun {
                count: 7,
                size: 128 * KIB,
            },
        ];
        // F401xC: 256 KiB.
        assert_eq!(
            truncate_runs(&f4, 256 * KIB),
            Some(vec![
                SectorRun {
                    count: 4,
                    size: 16 * KIB
                },
                SectorRun {
                    count: 1,
                    size: 64 * KIB
                },
                SectorRun {
                    count: 1,
                    size: 128 * KIB
                },
            ])
        );
        assert_eq!(truncate_runs(&f4, 1024 * KIB).map(|r| r.len()), Some(3));
        assert_eq!(truncate_runs(&f4, 2048 * KIB), None, "template too small");
        assert_eq!(
            truncate_runs(&f4, 100 * KIB),
            None,
            "not on a sector boundary"
        );
    }

    #[test]
    fn describe_layout() {
        assert_eq!(
            f427().describe(),
            "sectors 4 × 16 KiB, 1 × 64 KiB, 7 × 128 KiB | 4 × 16 KiB, 1 × 64 KiB, 7 × 128 KiB"
        );
        let f103 = FlashGeometry::uniform(0x0800_0000, 64 * KIB, 1, KIB, EraseUnit::Page, 2);
        assert_eq!(f103.describe(), "pages 64 × 1 KiB");
    }

    #[test]
    fn padding() {
        assert_eq!(pad(&[1, 2], 4, 0xFF), vec![1, 2, 0xFF, 0xFF]);
        assert_eq!(pad(&[1, 2], 2, 0xFF), vec![1, 2]);
    }
}
