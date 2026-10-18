//! USB discovery of ST-Link probes.

pub(crate) mod transport;

use std::fmt;
use std::time::Duration;

use nusb::{DeviceInfo, MaybeFuture};
use tracing::debug;

use crate::Result;

/// STMicroelectronics USB vendor ID.
pub const ST_VID: u16 = 0x0483;

/// Timeout for control transfers made during discovery.
const DISCOVERY_TIMEOUT: Duration = Duration::from_millis(500);

/// ST-Link hardware generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeModel {
    V2,
    V2_1,
    V3,
}

impl fmt::Display for ProbeModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ProbeModel::V2 => "ST-Link V2",
            ProbeModel::V2_1 => "ST-Link V2-1",
            ProbeModel::V3 => "ST-Link V3",
        })
    }
}

struct KnownProbe {
    pid: u16,
    model: ProbeModel,
    ep_out: u8,
    ep_in: u8,
}

const KNOWN_PROBES: &[KnownProbe] = &[
    KnownProbe {
        pid: 0x3748,
        model: ProbeModel::V2,
        ep_out: 0x02,
        ep_in: 0x81,
    },
    KnownProbe {
        pid: 0x374B,
        model: ProbeModel::V2_1,
        ep_out: 0x01,
        ep_in: 0x81,
    },
    KnownProbe {
        pid: 0x3752,
        model: ProbeModel::V2_1,
        ep_out: 0x01,
        ep_in: 0x81,
    },
    KnownProbe {
        pid: 0x374E,
        model: ProbeModel::V3,
        ep_out: 0x01,
        ep_in: 0x81,
    },
    KnownProbe {
        pid: 0x374F,
        model: ProbeModel::V3,
        ep_out: 0x01,
        ep_in: 0x81,
    },
    KnownProbe {
        pid: 0x3753,
        model: ProbeModel::V3,
        ep_out: 0x01,
        ep_in: 0x81,
    },
    KnownProbe {
        pid: 0x3754,
        model: ProbeModel::V3,
        ep_out: 0x01,
        ep_in: 0x81,
    },
];

/// A connected ST-Link found by [`list_probes`]; the device is not opened.
#[derive(Debug, Clone)]
pub struct ProbeDescriptor {
    pub vid: u16,
    pub pid: u16,
    pub model: ProbeModel,
    /// Serial number, normalized like `st-info` (raw-byte serials of clones as hex).
    pub serial: String,
    pub bus_id: String,
    pub device_address: u8,
    pub(crate) ep_out: u8,
    pub(crate) ep_in: u8,
    pub(crate) info: DeviceInfo,
}

/// Enumerate connected ST-Link probes. No devices → empty list.
pub fn list_probes() -> Result<Vec<ProbeDescriptor>> {
    let mut probes = Vec::new();
    for info in nusb::list_devices().wait()? {
        if info.vendor_id() != ST_VID {
            continue;
        }
        let Some(known) = KNOWN_PROBES.iter().find(|k| k.pid == info.product_id()) else {
            continue;
        };
        let serial = resolve_serial(&info);
        debug!(
            pid = format_args!("{:04x}", info.product_id()),
            bus = info.bus_id(),
            addr = info.device_address(),
            serial,
            "found {}",
            known.model
        );
        probes.push(ProbeDescriptor {
            vid: info.vendor_id(),
            pid: info.product_id(),
            model: known.model,
            serial,
            bus_id: info.bus_id().to_owned(),
            device_address: info.device_address(),
            ep_out: known.ep_out,
            ep_in: known.ep_in,
            info,
        });
    }
    Ok(probes)
}

/// Serial from the OS-cached string; if it is not usable, read the raw descriptor.
///
/// macOS replaces the raw-byte serial of some clones with `"?"`, so a string
/// that is not a full-length serial is only a last resort.
fn resolve_serial(info: &DeviceInfo) -> String {
    let cached = info.serial_number().and_then(serial_from_string);
    if let Some(s) = cached.as_ref().filter(|s| s.len() == SERIAL_LEN) {
        return s.clone();
    }
    let raw = match read_raw_serial_descriptor(info) {
        Ok(raw) => raw.and_then(|raw| serial_from_descriptor(&raw)),
        Err(e) => {
            debug!("cannot read serial descriptor: {e}");
            None
        }
    };
    raw.or(cached).unwrap_or_default()
}

fn read_raw_serial_descriptor(info: &DeviceInfo) -> Result<Option<Vec<u8>>> {
    let device = info.open().wait()?;
    let Some(index) = device.device_descriptor().serial_number_string_index() else {
        return Ok(None);
    };
    let lang = device
        .get_string_descriptor_supported_languages(DISCOVERY_TIMEOUT)
        .wait()
        .ok()
        .and_then(|mut langs| langs.next())
        .unwrap_or(nusb::descriptors::language_id::US_ENGLISH);
    Ok(device
        .get_descriptor(0x03, index.get(), lang, DISCOVERY_TIMEOUT)
        .wait()
        .ok())
}

/// Number of characters in a normal ST-Link serial.
const SERIAL_LEN: usize = 24;

/// Normalize a serial already decoded to a string by the OS.
///
/// Clones report 12 raw bytes as 12 UTF-16 code units; like `st-info`, they
/// become 24 hex digits made of the low byte of each unit.
fn serial_from_string(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() == SERIAL_LEN / 2 && !chars.contains(&char::REPLACEMENT_CHARACTER) {
        return Some(
            chars
                .iter()
                .map(|&c| format!("{:02X}", u32::from(c) & 0xFF))
                .collect(),
        );
    }
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_graphic()) {
        return Some(s.to_owned());
    }
    None
}

/// Normalize a raw USB string descriptor, following stlink `stlink_serial()`.
fn serial_from_descriptor(desc: &[u8]) -> Option<String> {
    let len = usize::from(*desc.first()?);
    let units = desc.get(2..len)?;
    match len {
        // Proper serial: 24 UTF-16 units → ASCII.
        50 => Some(
            units
                .chunks_exact(2)
                .map(|u| {
                    let unit = u16::from_le_bytes([u[0], u[1]]);
                    if unit < 0x80 { char::from(u[0]) } else { '?' }
                })
                .collect(),
        ),
        // Buggy (clone) serial: 12 units → low bytes as hex.
        26 => Some(
            units
                .iter()
                .step_by(2)
                .map(|b| format!("{b:02X}"))
                .collect(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_serial_is_kept() {
        assert_eq!(
            serial_from_string("066DFF555185754867093132").as_deref(),
            Some("066DFF555185754867093132")
        );
    }

    #[test]
    fn raw_byte_serial_string_becomes_hex() {
        let s: String = [
            0x55u32, 0xFF06, 0x06, 0x4F, 0x49, 0x3F, 0x89, 0x50, 0x48, 0x54, 0x23, 0x87,
        ]
        .iter()
        .filter_map(|&u| char::from_u32(u))
        .collect();
        assert_eq!(
            serial_from_string(&s).as_deref(),
            Some("5506064F493F895048542387")
        );
    }

    #[test]
    fn unprintable_serial_is_rejected() {
        assert_eq!(serial_from_string("ab\u{1}cd"), None);
        assert_eq!(serial_from_string(""), None);
    }

    #[test]
    fn descriptor_good_serial() {
        let mut desc = vec![50u8, 3];
        for c in "0670FF485550755187182844".bytes() {
            desc.extend_from_slice(&[c, 0]);
        }
        assert_eq!(
            serial_from_descriptor(&desc).as_deref(),
            Some("0670FF485550755187182844")
        );
    }

    #[test]
    fn descriptor_buggy_serial() {
        let mut desc = vec![26u8, 3];
        for b in [
            0x55u8, 0x06, 0x06, 0x4F, 0x49, 0x3F, 0x89, 0x50, 0x48, 0x54, 0x23, 0x87,
        ] {
            desc.extend_from_slice(&[b, 0xFF]);
        }
        assert_eq!(
            serial_from_descriptor(&desc).as_deref(),
            Some("5506064F493F895048542387")
        );
    }

    #[test]
    fn descriptor_bad_length() {
        assert_eq!(serial_from_descriptor(&[10, 3, 0, 0]), None);
        assert_eq!(serial_from_descriptor(&[]), None);
        assert_eq!(serial_from_descriptor(&[50, 3, 1, 2]), None);
    }
}
