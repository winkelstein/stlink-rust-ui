//! A chip's flash geometry from the chip table and `chips/layouts.txt`.

use super::geometry::{EraseUnit, FlashGeometry, SectorRun, truncate_runs};
use crate::chips::{self, BankMode, ChipDescriptor, LayoutTemplate};
use crate::{Error, Result};

/// Build the geometry of `chip` with `flash_size` bytes. `dual` is the bank
/// mode read from the option bytes, for chips that have one; without it such a
/// chip is refused (guessing would erase the wrong sectors).
pub(crate) fn build(
    chip: &ChipDescriptor,
    flash_size: u32,
    dual: Option<bool>,
    unit: EraseUnit,
    write_granularity: u32,
) -> Result<FlashGeometry> {
    let templates = chips::layouts(chip.chip_id);
    if templates.is_empty() {
        let banks = if chip.dual_bank { 2 } else { 1 };
        let page = chip.flash_pagesize;
        if page == 0 || flash_size == 0 || flash_size % (banks * page) != 0 {
            return Err(Error::Unsupported("flash layout of this chip"));
        }
        return Ok(FlashGeometry::uniform(
            chips::FLASH_BASE,
            flash_size,
            banks,
            page,
            unit,
            write_granularity,
        ));
    }

    let has_modes = templates.iter().any(|t| t.mode == BankMode::Dual)
        && templates.iter().any(|t| t.mode == BankMode::Single);
    if has_modes && dual.is_none() && chips::bank_mode_register(chip.chip_id).is_some() {
        return Err(Error::Unsupported(
            "flash layout: the bank mode could not be read from the option bytes",
        ));
    }
    let wanted = if dual == Some(true) {
        BankMode::Dual
    } else {
        BankMode::Single
    };
    // The configured mode first; the other one if it does not fit the size
    // (F42x 2 MiB parts are dual bank whatever DB1M says).
    let mut ordered: Vec<&LayoutTemplate> = templates.clone();
    ordered.sort_by_key(|t| t.mode != wanted && t.mode != BankMode::Fill);
    ordered
        .iter()
        .find_map(|t| apply(t, flash_size))
        .map(|banks| FlashGeometry::new(chips::FLASH_BASE, banks, unit, write_granularity))
        .ok_or(Error::Unsupported("flash layout for this flash size"))
}

fn apply(t: &LayoutTemplate, size: u32) -> Option<Vec<Vec<SectorRun>>> {
    match t.mode {
        BankMode::Single => Some(vec![truncate_runs(t.banks.first()?, size)?]),
        BankMode::Dual => {
            if size % 2 != 0 {
                return None;
            }
            t.banks.iter().map(|b| truncate_runs(b, size / 2)).collect()
        }
        BankMode::Fill => {
            let mut left = size;
            let mut out = Vec::new();
            for bank in &t.banks {
                if left == 0 {
                    break;
                }
                let bank_size: u32 = bank.iter().map(|r| r.count * r.size).sum();
                let take = bank_size.min(left);
                out.push(truncate_runs(bank, take)?);
                left -= take;
            }
            (left == 0).then_some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIB: u32 = 1024;

    fn chip(id: u16) -> &'static ChipDescriptor {
        chips::find(id).unwrap_or_else(|| unreachable!("chip 0x{id:03x} in the table"))
    }

    fn sizes(g: &FlashGeometry) -> Vec<(u32, u32)> {
        g.banks.iter().map(|b| (b.base, b.size())).collect()
    }

    #[test]
    fn uniform_pages_from_the_chip_file() {
        // F103C8: 64 KiB, 1 KiB pages.
        let g = build(chip(0x410), 64 * KIB, None, EraseUnit::Page, 2);
        assert_eq!(g.as_ref().map(|g| g.sectors().len()).ok(), Some(64));
        // H743: two banks of 128 KiB sectors.
        let g = build(chip(0x450), 2048 * KIB, None, EraseUnit::Sector, 32);
        assert_eq!(
            g.map(|g| sizes(&g)).ok(),
            Some(vec![(0x0800_0000, 1024 * KIB), (0x0810_0000, 1024 * KIB)])
        );
    }

    #[test]
    fn f4_templates_cut_to_size() {
        // F411CE: 512 KiB, one bank, 4 × 16 + 64 + 3 × 128.
        let g = build(chip(0x431), 512 * KIB, None, EraseUnit::Sector, 4);
        assert_eq!(g.as_ref().map(|g| g.sectors().len()).ok(), Some(8));
        assert_eq!(g.map(|g| g.size).ok(), Some(512 * KIB));
    }

    #[test]
    fn f42x_bank_modes() {
        let f427 = chip(0x419);
        // 2 MiB: always dual, whatever DB1M reads.
        let g = build(f427, 2048 * KIB, Some(false), EraseUnit::Sector, 4);
        assert_eq!(g.as_ref().map(|g| g.bank_count()).ok(), Some(2));
        assert_eq!(g.map(|g| g.sectors().len()).ok(), Some(24));
        // 1 MiB with DB1M: two banks of 512 KiB, bank 2 at 0x08080000.
        let g = build(f427, 1024 * KIB, Some(true), EraseUnit::Sector, 4);
        assert_eq!(
            g.map(|g| sizes(&g)).ok(),
            Some(vec![(0x0800_0000, 512 * KIB), (0x0808_0000, 512 * KIB)])
        );
        // 1 MiB single bank.
        let g = build(f427, 1024 * KIB, Some(false), EraseUnit::Sector, 4);
        assert_eq!(
            g.map(|g| (g.bank_count(), g.sectors().len())).ok(),
            Some((1, 12))
        );
        // Unknown bank mode: refused rather than guessed.
        assert!(matches!(
            build(f427, 1024 * KIB, None, EraseUnit::Sector, 4),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn f7_and_g4_and_f1_xl() {
        // F746: 1 MiB, 4 × 32 + 128 + 3 × 256.
        let g = build(chip(0x449), 1024 * KIB, None, EraseUnit::Sector, 4);
        assert_eq!(
            g.map(|g| g.sectors().iter().map(|s| s.size / KIB).collect::<Vec<_>>())
                .ok(),
            Some(vec![32, 32, 32, 32, 128, 256, 256, 256])
        );
        // F767 2 MiB dual (nDBANK clear).
        let g = build(chip(0x451), 2048 * KIB, Some(true), EraseUnit::Sector, 4);
        assert_eq!(g.map(|g| g.sectors().len()).ok(), Some(24));
        // G474 512 KiB: dual = 2 × 128 pages of 2 KiB, single = 128 pages of 4 KiB.
        let g = build(chip(0x469), 512 * KIB, Some(true), EraseUnit::Page, 8);
        assert_eq!(
            g.map(|g| (g.bank_count(), g.sectors().len())).ok(),
            Some((2, 256))
        );
        let g = build(chip(0x469), 512 * KIB, Some(false), EraseUnit::Page, 8);
        assert_eq!(
            g.map(|g| (g.bank_count(), g.sectors()[0].size)).ok(),
            Some((1, 4 * KIB))
        );
        // F103 XL, 768 KiB: bank 1 512 KiB, bank 2 256 KiB.
        let g = build(chip(0x430), 768 * KIB, None, EraseUnit::Page, 2);
        assert_eq!(
            g.map(|g| sizes(&g)).ok(),
            Some(vec![(0x0800_0000, 512 * KIB), (0x0808_0000, 256 * KIB)])
        );
    }
}
