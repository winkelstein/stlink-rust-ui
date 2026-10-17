//! Erase / program / verify sequences over any [`Memory`] and [`FlashDriver`].

use std::ops::ControlFlow;

use tracing::{debug, warn};

use super::geometry::WritePlan;
use super::mem::Memory;
use super::{FlashDriver, Progress, Stage};
use crate::{Error, Result};

/// Bytes programmed between progress callbacks.
const PROGRAM_BLOCK: usize = 4 * 1024;
/// Bytes read back between progress callbacks.
pub(crate) const READ_BLOCK: usize = 16 * 1024;

type OnProgress<'a> = &'a mut dyn FnMut(Progress) -> ControlFlow<()>;

fn report(f: OnProgress<'_>, stage: Stage, done: usize, total: usize) -> Result<()> {
    let p = Progress {
        stage,
        done: done as u64,
        total: total as u64,
    };
    if f(p).is_break() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

/// Halt, unlock the touched banks, erase the planned units, program `image`
/// (already padded), lock again (also on error), then optionally verify.
pub(crate) fn write(
    m: &mut dyn Memory,
    driver: &mut dyn FlashDriver,
    plan: &WritePlan,
    image: &[u8],
    verify: bool,
    on_progress: OnProgress<'_>,
) -> Result<()> {
    m.halt()?;
    let mut banks: Vec<u8> = plan.sectors.iter().map(|s| s.bank).collect();
    banks.dedup();
    let result = (|| {
        for &bank in &banks {
            driver.unlock(m, bank)?;
        }
        erase_and_program(m, driver, plan, image, on_progress)
    })();
    for &bank in &banks {
        if let Err(e) = driver.lock(m, bank) {
            warn!("cannot lock flash bank {}: {e}", bank + 1);
        }
    }
    result?;
    if verify {
        verify_image(m, plan.addr, image, on_progress)?;
    }
    Ok(())
}

fn erase_and_program(
    m: &mut dyn Memory,
    driver: &mut dyn FlashDriver,
    plan: &WritePlan,
    image: &[u8],
    on_progress: OnProgress<'_>,
) -> Result<()> {
    let n = plan.sectors.len();
    report(on_progress, Stage::Erasing, 0, n)?;
    for (i, sector) in plan.sectors.iter().enumerate() {
        debug!(
            "erase bank {} unit {} @0x{:08X} ({} bytes)",
            sector.bank + 1,
            sector.index,
            sector.addr,
            sector.size
        );
        driver.erase_sector(m, sector)?;
        report(on_progress, Stage::Erasing, i + 1, n)?;
    }

    let total = image.len();
    report(on_progress, Stage::Programming, 0, total)?;
    let mut done = 0;
    while done < total {
        let a = plan.addr + done as u32;
        // Never cross a bank boundary within one program call.
        let bank_end = driver
            .geometry()
            .bank_of(a)
            .map_or(u64::MAX, |b| driver.geometry().bank_end(b));
        let n = (total - done)
            .min(PROGRAM_BLOCK)
            .min((bank_end - u64::from(a)).min(usize::MAX as u64) as usize);
        driver.program(m, a, &image[done..done + n])?;
        done += n;
        report(on_progress, Stage::Programming, done, total)?;
    }
    Ok(())
}

/// Read back and compare (through the AHB-AP: caches on F7/H7 do not matter).
pub(crate) fn verify_image(
    m: &mut dyn Memory,
    addr: u32,
    image: &[u8],
    on_progress: OnProgress<'_>,
) -> Result<()> {
    let total = image.len();
    report(on_progress, Stage::Verifying, 0, total)?;
    let mut done = 0;
    while done < total {
        let n = (total - done).min(READ_BLOCK);
        let a = addr + done as u32;
        let got = m.read_block(a, n)?;
        if let Some(i) = got
            .iter()
            .zip(&image[done..done + n])
            .position(|(g, w)| g != w)
        {
            return Err(Error::Verify { addr: a + i as u32 });
        }
        done += n;
        report(on_progress, Stage::Verifying, done, total)?;
    }
    Ok(())
}

/// Erase every bank: halt, then unlock / erase / lock bank by bank.
pub(crate) fn mass_erase(
    m: &mut dyn Memory,
    driver: &mut dyn FlashDriver,
    on_progress: OnProgress<'_>,
) -> Result<()> {
    let banks = driver.geometry().bank_count();
    m.halt()?;
    erase_banks(banks, on_progress, |bank| {
        driver.unlock(m, bank)?;
        let r = driver.erase_bank(m, bank);
        if let Err(e) = driver.lock(m, bank) {
            warn!("cannot lock flash bank {}: {e}", bank + 1);
        }
        debug!("bank {} erased", bank + 1);
        r
    })
}

/// Erase banks `0..banks` in order, reporting progress before the first and
/// after each bank; a `Break` stops before the next bank.
pub(crate) fn erase_banks(
    banks: u8,
    on_progress: OnProgress<'_>,
    mut erase: impl FnMut(u8) -> Result<()>,
) -> Result<()> {
    report(on_progress, Stage::Erasing, 0, usize::from(banks))?;
    for bank in 0..banks {
        erase(bank)?;
        report(
            on_progress,
            Stage::Erasing,
            usize::from(bank) + 1,
            usize::from(banks),
        )?;
    }
    Ok(())
}
