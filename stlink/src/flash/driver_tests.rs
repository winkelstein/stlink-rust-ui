//! Driver tests against the register model (`fake`): for every family the
//! full write cycle (unlock → erase → program → verify → lock), mass erase,
//! SR errors mapped to typed errors, tail padding, bank boundaries, and the
//! misuse the model must catch.

use std::ops::ControlFlow;

use super::fake::{FakeFlash, Family};
use super::geometry::{Sector, pad};
use super::mem::Memory;
use super::{FlashDriver, driver_for, f4, ops};
use crate::{Error, Result, chips};

const KIB: u32 = 1024;

/// Driver and model of `chip_id` with `size` bytes; flash pre-filled with 0xA5
/// so erased and untouched units can be told apart.
fn setup(
    chip_id: u16,
    size: u32,
    dual: Option<bool>,
    voltage: Option<f32>,
) -> Result<(Box<dyn FlashDriver>, FakeFlash)> {
    let chip = chips::find(chip_id).ok_or(Error::UnknownChip(u32::from(chip_id)))?;
    let driver = driver_for(chip, size, dual, voltage)?;
    let family = match chip.flash_type.as_str() {
        "F0_F1_F3" | "F1_XL" => Family::F0F1,
        "F2_F4" | "F7" => Family::F4,
        "G4" => Family::G4,
        _ => Family::H7,
    };
    let fake = FakeFlash::new(family, driver.geometry().clone(), 0xA5);
    Ok((driver, fake))
}

fn image(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 + 3) as u8 | 1).collect()
}

fn go(_: super::Progress) -> ControlFlow<()> {
    ControlFlow::Continue(())
}

/// Write `len` bytes at `addr` with verify and check the whole flash after.
fn write_and_check(
    chip_id: u16,
    size: u32,
    dual: Option<bool>,
    addr: u32,
    len: usize,
) -> Result<Vec<Sector>> {
    let (mut d, mut f) = setup(chip_id, size, dual, None)?;
    let data = image(len);
    let plan = d.geometry().plan_write(addr, len)?;
    let img = pad(&data, plan.padded_len as usize, 0xFF);
    ops::write(&mut f, d.as_mut(), &plan, &img, true, &mut go)?;
    assert!(f.violations.is_empty(), "{:#?}", f.violations);
    assert!(f.locked(), "locked again");
    assert!(f.halted, "core halted");
    assert_eq!(
        f.erased,
        plan.sectors.iter().map(|s| s.addr).collect::<Vec<_>>()
    );

    let g = d.geometry().clone();
    let start = (addr - g.base) as usize;
    let end = start + plan.padded_len as usize;
    assert_eq!(&f.flash[start..start + len], &data[..], "programmed data");
    assert!(
        f.flash[start + len..end].iter().all(|&b| b == 0xFF),
        "tail padded with 0xFF"
    );
    for s in g.sectors() {
        let (a, b) = (
            (s.addr - g.base) as usize,
            (s.addr - g.base + s.size) as usize,
        );
        let erased = plan.sectors.contains(&s);
        for (i, &byte) in f.flash[a..b].iter().enumerate() {
            let at = a + i;
            if (start..end).contains(&at) {
                continue;
            }
            assert_eq!(
                byte,
                if erased { 0xFF } else { 0xA5 },
                "byte 0x{:08X}",
                g.base as usize + at
            );
        }
    }
    Ok(plan.sectors)
}

fn mass_erase_check(chip_id: u16, size: u32, dual: Option<bool>) -> Result<usize> {
    let (mut d, mut f) = setup(chip_id, size, dual, None)?;
    let mut reports = Vec::new();
    ops::mass_erase(&mut f, d.as_mut(), &mut |p| {
        reports.push((p.done, p.total));
        ControlFlow::Continue(())
    })?;
    assert!(f.violations.is_empty(), "{:#?}", f.violations);
    assert!(f.flash.iter().all(|&b| b == 0xFF), "everything erased");
    assert!(f.locked());
    let banks = d.geometry().bank_count();
    assert_eq!(reports.last(), Some(&(u64::from(banks), u64::from(banks))));
    Ok(f.erased.len())
}

/// Inject `bits` into the next erase and return the driver's error.
fn erase_error(chip_id: u16, size: u32, dual: Option<bool>, bits: u32) -> Result<Error> {
    let (mut d, mut f) = setup(chip_id, size, dual, None)?;
    let sector = d.geometry().sectors()[0];
    d.unlock(&mut f, 0)?;
    f.fail_next = Some(bits);
    Ok(match d.erase_sector(&mut f, &sector) {
        Err(e) => e,
        Ok(()) => Error::InvalidArgument("no error"),
    })
}

/// Program the same range twice without erasing: the model must refuse.
fn reprogram_is_caught(chip_id: u16, size: u32, dual: Option<bool>) -> Result<()> {
    let (mut d, mut f) = setup(chip_id, size, dual, None)?;
    f.halt()?; // as `ops::write` does
    let s = d.geometry().sectors()[0];
    d.unlock(&mut f, 0)?;
    d.erase_sector(&mut f, &s)?;
    let data = image(64);
    d.program(&mut f, s.addr, &data)?;
    assert!(f.violations.is_empty(), "{:#?}", f.violations);
    let again = d.program(&mut f, s.addr, &data);
    assert!(again.is_err(), "second program must fail");
    assert!(
        f.violations.iter().any(|v| v.contains("non-blank")),
        "{:#?}",
        f.violations
    );
    Ok(())
}

/// Programming without unlocking: the controller ignores CR, the model
/// complains, the driver reports the error flag.
fn program_locked_is_caught(chip_id: u16, size: u32, dual: Option<bool>) -> Result<()> {
    let (mut d, mut f) = setup(chip_id, size, dual, None)?;
    f.halt()?; // as `ops::write` does
    let s = d.geometry().sectors()[0];
    let r = d.program(&mut f, s.addr, &image(64));
    assert!(r.is_err());
    assert!(!f.violations.is_empty());
    Ok(())
}

// ---- F0 / F1 / F3 ----

#[test]
fn f103c8_write_with_odd_length() -> Result<()> {
    // 64 KiB, 1 KiB pages: 2049 bytes → 3 pages, padded to 2050.
    let s = write_and_check(0x410, 64 * KIB, None, 0x0800_0400, 2049)?;
    assert_eq!(s.iter().map(|s| s.index).collect::<Vec<_>>(), vec![1, 2, 3]);
    Ok(())
}

#[test]
fn f103c8_programs_through_the_loader() -> Result<()> {
    let (mut d, mut f) = setup(0x410, 64 * KIB, None, None)?;
    let mut data = image(7000);
    data[4096..6144].fill(0xFF); // a blank buffer's worth is skipped
    let plan = d.geometry().plan_write(0x0800_0000, data.len())?;
    let img = pad(&data, plan.padded_len as usize, 0xFF);
    ops::write(&mut f, d.as_mut(), &plan, &img, true, &mut go)?;
    assert!(f.violations.is_empty(), "{:#?}", f.violations);
    assert_eq!(&f.flash[..data.len()], &data[..]);
    // 2 KiB per loader run: 0–2K, 2K–4K, (4K–6K blank), 6K–7000.
    assert_eq!(f.routines, 3);
    Ok(())
}

#[test]
fn f103c8_mass_erase_and_errors() -> Result<()> {
    assert_eq!(mass_erase_check(0x410, 64 * KIB, None)?, 1);
    assert!(
        matches!(
            erase_error(0x410, 64 * KIB, None, 1 << 4)?,
            Error::FlashProtected
        ),
        "WRPRTERR"
    );
    assert!(
        matches!(
            erase_error(0x410, 64 * KIB, None, 1 << 2)?,
            Error::FlashOperation { .. }
        ),
        "PGERR"
    );
    reprogram_is_caught(0x410, 64 * KIB, None)?;
    program_locked_is_caught(0x410, 64 * KIB, None)
}

#[test]
fn f0_and_f3_use_the_same_controller() -> Result<()> {
    write_and_check(0x440, 32 * KIB, None, 0x0800_0000, 1500)?; // F05x, 1 KiB pages
    write_and_check(0x422, 256 * KIB, None, 0x0800_1000, 6000)?; // F30x, 2 KiB pages
    Ok(())
}

#[test]
fn f1_xl_writes_across_banks() -> Result<()> {
    // 768 KiB: bank 1 ends at 0x08080000; second register set for bank 2.
    let s = write_and_check(0x430, 768 * KIB, None, 0x0807_F800, 4096)?;
    assert_eq!(s.iter().map(|s| s.bank).collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(mass_erase_check(0x430, 768 * KIB, None)?, 2);
    Ok(())
}

// ---- F2 / F4 ----

#[test]
fn f411_non_uniform_sectors() -> Result<()> {
    // 70 KiB from the start: four 16 KiB sectors and the 64 KiB one.
    let s = write_and_check(0x431, 512 * KIB, None, 0x0800_0000, 70 * 1024)?;
    assert_eq!(
        s.iter().map(|s| s.size / KIB).collect::<Vec<_>>(),
        vec![16, 16, 16, 16, 64]
    );
    assert_eq!(mass_erase_check(0x431, 512 * KIB, None)?, 1);
    reprogram_is_caught(0x431, 512 * KIB, None)?;
    program_locked_is_caught(0x431, 512 * KIB, None)
}

#[test]
fn f411_errors_and_bad_snb() -> Result<()> {
    assert!(
        matches!(
            erase_error(0x431, 512 * KIB, None, 1 << 4)?,
            Error::FlashProtected
        ),
        "WRPERR"
    );
    assert!(
        matches!(
            erase_error(0x431, 512 * KIB, None, 1 << 7)?,
            Error::FlashOperation { .. }
        ),
        "PGSERR"
    );
    // Sector 12 does not exist on a single-bank F411.
    let (mut d, mut f) = setup(0x431, 512 * KIB, None, None)?;
    d.unlock(&mut f, 0)?;
    let bogus = Sector {
        bank: 1,
        index: 0,
        addr: 0x0808_0000,
        size: 16 * KIB,
    };
    assert!(d.erase_sector(&mut f, &bogus).is_err());
    assert!(
        f.violations.iter().any(|v| v.contains("SNB")),
        "{:#?}",
        f.violations
    );
    Ok(())
}

#[test]
fn f427_dual_bank_2mib() -> Result<()> {
    // Across the bank boundary: sector 11 of bank 1, then 12–16 (SNB 0x10–0x14).
    let s = write_and_check(0x419, 2048 * KIB, Some(false), 0x080E_0000, 256 * 1024)?;
    assert_eq!(
        s.iter().map(f4::SectorFlash::snb).collect::<Vec<_>>(),
        vec![11, 0x10, 0x11, 0x12, 0x13, 0x14]
    );
    assert_eq!(mass_erase_check(0x419, 2048 * KIB, Some(false))?, 2);
    Ok(())
}

#[test]
fn f427_1mib_db1m() -> Result<()> {
    // DB1M: bank 2 at 0x08080000 is sector 12.
    let s = write_and_check(0x419, 1024 * KIB, Some(true), 0x0808_0000, 100)?;
    assert_eq!(
        s.iter().map(f4::SectorFlash::snb).collect::<Vec<_>>(),
        vec![0x10]
    );
    // Single bank: 0x08080000 is sector 8 (128 KiB).
    let s = write_and_check(0x419, 1024 * KIB, Some(false), 0x0808_0000, 100)?;
    assert_eq!(
        s.iter()
            .map(|s| (s.index, s.size / KIB))
            .collect::<Vec<_>>(),
        vec![(8, 128)]
    );
    Ok(())
}

#[test]
fn f4_below_2v7_programs_bytes() -> Result<()> {
    let (mut d, mut f) = setup(0x431, 512 * KIB, None, Some(2.1))?;
    assert_eq!(d.geometry().write_granularity, 1);
    let plan = d.geometry().plan_write(0x0800_0001, 333)?;
    let img = image(333);
    ops::write(&mut f, d.as_mut(), &plan, &img, true, &mut go)?;
    assert!(f.violations.is_empty(), "{:#?}", f.violations);
    assert_eq!(f4::Psize::for_voltage(Some(3.3)), f4::Psize::X32);
    assert_eq!(f4::Psize::for_voltage(None), f4::Psize::X32);
    assert_eq!(f4::Psize::for_voltage(Some(1.8)), f4::Psize::X8);
    Ok(())
}

#[test]
fn f2_uses_the_f4_controller() -> Result<()> {
    write_and_check(0x411, 1024 * KIB, None, 0x0800_C000, 20_000)?;
    Ok(())
}

// ---- F7 ----

#[test]
fn f746_sectors() -> Result<()> {
    // 32 KiB sectors 1 and 2.
    let s = write_and_check(0x449, 1024 * KIB, None, 0x0800_8000, 40 * 1024)?;
    assert_eq!(
        s.iter()
            .map(|s| (s.index, s.size / KIB))
            .collect::<Vec<_>>(),
        vec![(1, 32), (2, 32)]
    );
    assert_eq!(mass_erase_check(0x449, 1024 * KIB, None)?, 1);
    reprogram_is_caught(0x449, 1024 * KIB, None)
}

#[test]
fn f767_dual_and_single_bank() -> Result<()> {
    // nDBANK clear: dual, bank 2 at 0x08100000 starts with 16 KiB sectors.
    let s = write_and_check(0x451, 2048 * KIB, Some(true), 0x0810_0000, 20 * 1024)?;
    assert_eq!(
        s.iter().map(f4::SectorFlash::snb).collect::<Vec<_>>(),
        vec![0x10, 0x11]
    );
    assert_eq!(mass_erase_check(0x451, 2048 * KIB, Some(true))?, 2);
    // Single bank: 256 KiB sectors at the top.
    let s = write_and_check(0x451, 2048 * KIB, Some(false), 0x0810_0000, 20 * 1024)?;
    assert_eq!(
        s.iter()
            .map(|s| (s.index, s.size / KIB))
            .collect::<Vec<_>>(),
        vec![(8, 256)]
    );
    Ok(())
}

#[test]
fn f72x_sectors() -> Result<()> {
    write_and_check(0x452, 512 * KIB, None, 0x0802_0000, 64 * 1024)?;
    Ok(())
}

// ---- G4 ----

#[test]
fn g474_dual_bank_across_the_boundary() -> Result<()> {
    // Dual bank: 2 KiB pages, bank 2 at 0x08040000 (BKER).
    let s = write_and_check(0x469, 512 * KIB, Some(true), 0x0803_F800, 4096)?;
    assert_eq!(
        s.iter().map(|s| (s.bank, s.index)).collect::<Vec<_>>(),
        vec![(0, 127), (1, 0)]
    );
    assert_eq!(mass_erase_check(0x469, 512 * KIB, Some(true))?, 2);
    Ok(())
}

#[test]
fn g474_single_bank_4k_pages() -> Result<()> {
    // 13 bytes → padded to 16 (two double words).
    let s = write_and_check(0x469, 512 * KIB, Some(false), 0x0804_0000, 13)?;
    assert_eq!(
        s.iter()
            .map(|s| (s.index, s.size / KIB))
            .collect::<Vec<_>>(),
        vec![(64, 4)]
    );
    assert_eq!(mass_erase_check(0x469, 512 * KIB, Some(false))?, 1);
    Ok(())
}

#[test]
fn g4_errors_and_alignment() -> Result<()> {
    assert!(
        matches!(
            erase_error(0x468, 128 * KIB, None, 1 << 4)?,
            Error::FlashProtected
        ),
        "WRPERR"
    );
    for bit in [1, 3, 5, 6, 7, 8, 9] {
        assert!(
            matches!(
                erase_error(0x468, 128 * KIB, None, 1 << bit)?,
                Error::FlashOperation { .. }
            ),
            "SR bit {bit}"
        );
    }
    let (mut d, mut f) = setup(0x468, 128 * KIB, None, None)?;
    d.unlock(&mut f, 0)?;
    assert!(matches!(
        d.program(&mut f, 0x0800_0004, &[0; 8]),
        Err(Error::InvalidArgument(_))
    ));
    reprogram_is_caught(0x468, 128 * KIB, None)?;
    program_locked_is_caught(0x468, 128 * KIB, None)
}

#[test]
fn g4_model_rejects_single_words() -> Result<()> {
    // The model enforces double words even if a driver forgot.
    use super::mem::Memory;
    let (mut d, mut f) = setup(0x468, 128 * KIB, None, None)?;
    let s = d.geometry().sectors()[0];
    d.unlock(&mut f, 0)?;
    d.erase_sector(&mut f, &s)?;
    f.write_u32(0x4002_2014, 1)?; // CR = PG
    f.write_block32(0x0800_0000, &[0; 4])?;
    assert!(
        f.violations
            .iter()
            .any(|v| v.contains("32-bit write of 4 bytes")),
        "{:#?}",
        f.violations
    );
    Ok(())
}

// ---- H7 (regression) ----

#[test]
fn h743_across_banks_and_mass_erase() -> Result<()> {
    let s = write_and_check(0x450, 2048 * KIB, None, 0x080E_0000, 256 * 1024)?;
    assert_eq!(
        s.iter().map(|s| (s.bank, s.index)).collect::<Vec<_>>(),
        vec![(0, 7), (1, 0)]
    );
    assert_eq!(mass_erase_check(0x450, 2048 * KIB, None)?, 2);
    assert!(
        matches!(
            erase_error(0x450, 2048 * KIB, None, 1 << 17)?,
            Error::FlashProtected
        ),
        "WRPERR"
    );
    assert!(
        matches!(
            erase_error(0x450, 2048 * KIB, None, 1 << 18)?,
            Error::FlashOperation { .. }
        ),
        "PGSERR"
    );
    reprogram_is_caught(0x450, 2048 * KIB, None)?;
    program_locked_is_caught(0x450, 2048 * KIB, None)
}

// ---- model and sequences ----

#[test]
fn wrong_key_sequence_locks_until_reset() -> Result<()> {
    use super::mem::Memory;
    let (mut d, mut f) = setup(0x410, 64 * KIB, None, None)?;
    f.write_u32(0x4002_2004, 0x1234_5678)?; // garbage into KEYR
    assert!(matches!(d.unlock(&mut f, 0), Err(Error::FlashProtected)));
    assert!(f.violations.iter().any(|v| v.contains("wrong key")));
    Ok(())
}

#[test]
fn option_registers_are_never_written() -> Result<()> {
    // A full write and mass erase touch no option register.
    let (mut d, mut f) = setup(0x419, 2048 * KIB, Some(false), None)?;
    f.opt.insert(0x4002_3C14, 0x0FFF_AAED);
    let plan = d.geometry().plan_write(0x0800_0000, 1000)?;
    let img = pad(&image(1000), plan.padded_len as usize, 0xFF);
    ops::write(&mut f, d.as_mut(), &plan, &img, true, &mut go)?;
    ops::mass_erase(&mut f, d.as_mut(), &mut go)?;
    assert!(f.violations.is_empty(), "{:#?}", f.violations);
    assert_eq!(f.opt.get(&0x4002_3C14), Some(&0x0FFF_AAED));
    Ok(())
}

#[test]
fn cancel_between_units_leaves_the_flash_locked() -> Result<()> {
    let (mut d, mut f) = setup(0x410, 64 * KIB, None, None)?;
    let plan = d.geometry().plan_write(0x0800_0000, 8 * 1024)?;
    let img = pad(&image(8 * 1024), plan.padded_len as usize, 0xFF);
    let r = ops::write(&mut f, d.as_mut(), &plan, &img, false, &mut |p| {
        if p.stage == super::Stage::Erasing && p.done == 3 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(matches!(r, Err(Error::Cancelled)));
    assert_eq!(f.erased.len(), 3);
    assert!(f.locked());
    Ok(())
}
