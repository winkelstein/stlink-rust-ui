//! Memory access used by the flash drivers: `StLink` on hardware, `FakeFlash`
//! in tests. Writes are never retried on WAIT: re-sending a flash write would
//! program the same words twice.

use std::time::Duration;

use crate::Result;
use crate::probe::{ProbeCapabilities, StLink};

pub(crate) trait Memory {
    fn read_u32(&mut self, addr: u32) -> Result<u32>;
    fn write_u32(&mut self, addr: u32, value: u32) -> Result<()>;
    /// 32-bit bus writes; `addr` and `data.len()` 4-aligned, any length.
    fn write_block32(&mut self, addr: u32, data: &[u8]) -> Result<()>;
    /// 8-bit bus writes (F2/F4/F7 programming below 2.7 V).
    fn write_block8(&mut self, addr: u32, data: &[u8]) -> Result<()>;
    fn read_block(&mut self, addr: u32, len: usize) -> Result<Vec<u8>>;
    fn halt(&mut self) -> Result<()>;
    /// Run a routine loaded in target RAM on the halted core until it halts;
    /// returns R0 and R1 (see `StLink::run_routine`).
    fn run_routine(
        &mut self,
        entry: u32,
        args: [u32; 4],
        sp: u32,
        timeout: Duration,
    ) -> Result<[u32; 2]>;
    /// Pause between status polls (the fake does not wait).
    fn delay(&mut self, d: Duration);
}

/// Split `[addr, addr + len)` into chunks of at most `max` bytes that do not
/// cross a `max`-aligned boundary (the AP's TAR auto-increment window).
fn chunks(addr: u32, len: usize, max: usize) -> impl Iterator<Item = (u32, usize, usize)> {
    let max = max.max(1) as u64;
    let end = u64::from(addr) + len as u64;
    let mut a = u64::from(addr);
    std::iter::from_fn(move || {
        if a >= end {
            return None;
        }
        let window_end = (a / max + 1) * max;
        let n = window_end.min(end) - a;
        let item = (a as u32, (a - u64::from(addr)) as usize, n as usize);
        a += n;
        Some(item)
    })
}

impl Memory for StLink {
    fn read_u32(&mut self, addr: u32) -> Result<u32> {
        StLink::read_u32(self, addr)
    }

    fn write_u32(&mut self, addr: u32, value: u32) -> Result<()> {
        StLink::write_u32(self, addr, value)
    }

    fn write_block32(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        for (a, off, n) in chunks(addr, data.len(), ProbeCapabilities::MAX_RW32) {
            self.write_mem32(a, &data[off..off + n])?;
        }
        Ok(())
    }

    fn write_block8(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        let max = self.capabilities().max_rw8;
        for (a, off, n) in chunks(addr, data.len(), max) {
            self.write_mem8(a, &data[off..off + n])?;
        }
        Ok(())
    }

    fn read_block(&mut self, addr: u32, len: usize) -> Result<Vec<u8>> {
        self.read_memory(addr, len)
    }

    fn halt(&mut self) -> Result<()> {
        StLink::halt(self)
    }

    fn run_routine(
        &mut self,
        entry: u32,
        args: [u32; 4],
        sp: u32,
        timeout: Duration,
    ) -> Result<[u32; 2]> {
        StLink::run_routine(self, entry, args, sp, timeout)
    }

    fn delay(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}

#[cfg(test)]
mod tests {
    use super::chunks;

    #[test]
    fn chunks_stay_in_the_window() {
        let c: Vec<_> = chunks(0x0800_03F0, 0x420, 1024).collect();
        assert_eq!(
            c,
            vec![
                (0x0800_03F0, 0, 0x10),
                (0x0800_0400, 0x10, 0x400),
                (0x0800_0800, 0x410, 0x10)
            ]
        );
        assert_eq!(chunks(0, 0, 1024).count(), 0);
    }
}
