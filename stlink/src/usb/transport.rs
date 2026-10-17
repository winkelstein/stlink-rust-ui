//! Bulk transport to the ST-Link debug interface, with packet tracing.

use std::fmt::Write as _;
use std::thread;
use std::time::{Duration, Instant};

use nusb::transfer::{Buffer, Bulk, In, Out, TransferError};
use nusb::{DeviceId, Endpoint, Interface, MaybeFuture};
use tracing::{debug, trace};

use super::ProbeDescriptor;
use crate::{Error, Result};

/// ST-Link command packet size.
pub(crate) const CMD_LEN: usize = 16;

pub(crate) struct Transport {
    out: Endpoint<Bulk, Out>,
    inp: Endpoint<Bulk, In>,
    timeout: Duration,
    device_id: DeviceId,
    /// Set once the device is known to be unplugged: fail fast afterwards.
    gone: bool,
    // Kept last so the interface is released after the endpoints.
    _interface: Interface,
}

impl Transport {
    pub(crate) fn open(desc: &ProbeDescriptor, timeout: Duration) -> Result<Self> {
        let device = desc.info.open().wait()?;
        let interface = device.claim_interface(0).wait()?;
        let out = interface.endpoint::<Bulk, Out>(desc.ep_out)?;
        let inp = interface.endpoint::<Bulk, In>(desc.ep_in)?;
        Ok(Self {
            out,
            inp,
            timeout,
            device_id: desc.info.id(),
            gone: false,
            _interface: interface,
        })
    }

    /// Send a command (≤ 16 bytes, zero-padded) and read `resp_len` bytes back.
    pub(crate) fn command(&mut self, name: &str, cmd: &[u8], resp_len: usize) -> Result<Vec<u8>> {
        if cmd.len() > CMD_LEN {
            return Err(Error::InvalidArgument("command longer than 16 bytes"));
        }
        if self.gone {
            return Err(Error::Disconnected);
        }
        let mut packet = [0u8; CMD_LEN];
        packet[..cmd.len()].copy_from_slice(cmd);

        let start = Instant::now();
        trace!("→ {} ({name})", hex(cmd));
        self.write(&packet)?;
        if resp_len == 0 {
            return Ok(Vec::new());
        }
        let resp = self.read(resp_len)?;
        trace!("← {} ({name}, {:?})", hex(&resp), start.elapsed());
        Ok(resp)
    }

    /// Send the data phase of a write command.
    pub(crate) fn write_data(&mut self, name: &str, data: &[u8]) -> Result<()> {
        if self.gone {
            return Err(Error::Disconnected);
        }
        trace!("→ {} ({name} data)", hex(data));
        self.write(data)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        let c = self
            .out
            .transfer_blocking(Buffer::from(data.to_vec()), self.timeout);
        c.status.map_err(|e| self.transfer_error(e))?;
        if c.actual_len != data.len() {
            return Err(Error::InvalidResponse("short write"));
        }
        Ok(())
    }

    fn read(&mut self, len: usize) -> Result<Vec<u8>> {
        // IN requests must be a multiple of the max packet size; the probe
        // ends the transfer with a short packet.
        let mps = self.inp.max_packet_size().max(1);
        let c = self
            .inp
            .transfer_blocking(Buffer::new(len.div_ceil(mps) * mps), self.timeout);
        c.status.map_err(|e| self.transfer_error(e))?;
        let mut data = c.buffer.into_vec();
        if data.len() < len {
            trace!("← {} (short: {} of {len} bytes)", hex(&data), data.len());
            return Err(Error::InvalidResponse("short response"));
        }
        data.truncate(len);
        Ok(data)
    }
}

impl Transport {
    /// Map a transfer error, turning it into `Disconnected` if the device is
    /// gone. macOS reports an unplug mid-transfer as a stall (0xE000404F) or
    /// timeout rather than "no device", so check the bus.
    fn transfer_error(&mut self, err: TransferError) -> Error {
        let mapped = match err {
            // We never cancel transfers ourselves except on timeout.
            TransferError::Cancelled => Error::Timeout,
            other => other.into(),
        };
        if matches!(mapped, Error::Disconnected) || !self.device_present() {
            debug!("USB error {err:?}: device is gone");
            self.gone = true;
            return Error::Disconnected;
        }
        mapped
    }

    fn device_present(&self) -> bool {
        // The OS may still list a device for a moment after unplug.
        for attempt in 0..2 {
            if attempt > 0 {
                thread::sleep(Duration::from_millis(100));
            }
            let present = nusb::list_devices()
                .wait()
                .map(|mut devs| devs.any(|d| d.id() == self.device_id))
                // Enumeration itself failing: assume present, keep the original error.
                .unwrap_or(true);
            if !present {
                return false;
            }
        }
        true
    }
}

/// Hex dump for trace logs, truncated for long payloads.
pub(crate) fn hex(data: &[u8]) -> String {
    const MAX: usize = 64;
    let mut s = String::with_capacity(data.len().min(MAX) * 3 + 16);
    for (i, b) in data.iter().take(MAX).enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{b:02x}");
    }
    if data.len() > MAX {
        let _ = write!(s, " … ({} bytes)", data.len());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_short() {
        assert_eq!(hex(&[0xf1, 0x80, 0x00]), "f1 80 00");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn hex_truncates() {
        let s = hex(&[0u8; 100]);
        assert!(s.ends_with("… (100 bytes)"));
    }
}
