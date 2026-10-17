//! Helpers shared by the flash controller drivers.

use std::time::{Duration, Instant};

use tracing::debug;

use super::mem::Memory;
use crate::{Error, Result};

/// FLASH_KEYR unlock sequence (same on all STM32 families here).
pub(crate) const KEY1: u32 = 0x4567_0123;
pub(crate) const KEY2: u32 = 0xCDEF_89AB;

/// Pause between status polls during long operations.
const POLL: Duration = Duration::from_millis(5);

/// Unlock a controller: write the keys if `cr & lock_bit`, then check.
/// A controller that stays locked (wrong sequence before) needs a reset.
pub(crate) fn unlock_with_keys(
    m: &mut dyn Memory,
    keyr: u32,
    cr: u32,
    lock_bit: u32,
) -> Result<()> {
    if m.read_u32(cr)? & lock_bit == 0 {
        return Ok(());
    }
    m.write_u32(keyr, KEY1)?;
    m.write_u32(keyr, KEY2)?;
    if m.read_u32(cr)? & lock_bit != 0 {
        debug!("flash controller stays locked after the key sequence (CR 0x{cr:08X})");
        return Err(Error::FlashProtected);
    }
    Ok(())
}

/// Poll `sr` until none of `busy` is set; returns the last SR value.
/// Short operations (programming) poll without pausing.
pub(crate) fn wait_idle(m: &mut dyn Memory, sr: u32, busy: u32, timeout: Duration) -> Result<u32> {
    let start = Instant::now();
    loop {
        let v = m.read_u32(sr)?;
        if v & busy == 0 {
            return Ok(v);
        }
        if start.elapsed() > timeout {
            debug!("flash busy after {timeout:?}, SR 0x{v:08X}");
            return Err(Error::Timeout);
        }
        if timeout >= Duration::from_secs(1) {
            m.delay(POLL);
        }
    }
}

/// Map SR error bits: write protection → `FlashProtected`, anything else in
/// `errors` → `FlashOperation`.
pub(crate) fn check_errors(sr: u32, errors: u32, protection: u32) -> Result<()> {
    if sr & protection != 0 {
        return Err(Error::FlashProtected);
    }
    if sr & errors != 0 {
        return Err(Error::FlashOperation { status: sr });
    }
    Ok(())
}
