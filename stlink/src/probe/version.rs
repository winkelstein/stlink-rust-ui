//! ST-Link firmware version decoding and feature gating.

use std::fmt;

use crate::{Error, Result};

/// Debug API generation implemented by the probe firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiVersion {
    V1,
    V2,
    V3,
}

/// Firmware version of the probe. A component of `0` means "absent".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeVersion {
    pub stlink: u8,
    pub jtag: u8,
    pub swim: u8,
    /// Mass-storage firmware version (V3 only).
    pub msd: u8,
    /// Bridge firmware version (V3 only).
    pub bridge: u8,
    pub vid: u16,
    pub pid: u16,
    pub api: ApiVersion,
}

impl ProbeVersion {
    /// Decode the 6-byte `GET_VERSION` response.
    pub fn from_v1_response(r: &[u8]) -> Result<Self> {
        let r: &[u8; 6] = r
            .get(..6)
            .and_then(|s| s.try_into().ok())
            .ok_or(Error::InvalidResponse("version"))?;
        let [b0, b1, v0, v1, p0, p1] = *r;
        let stlink = b0 >> 4;
        let api = match stlink {
            1 => ApiVersion::V1,
            2 => ApiVersion::V2,
            _ => ApiVersion::V3,
        };
        Ok(Self {
            stlink,
            jtag: ((b0 & 0x0F) << 2) | (b1 >> 6),
            swim: b1 & 0x3F,
            msd: 0,
            bridge: 0,
            vid: u16::from_le_bytes([v0, v1]),
            pid: u16::from_le_bytes([p0, p1]),
            api,
        })
    }

    /// Decode the 12-byte `GET_VERSION_APIV3` response (ST-Link V3).
    pub fn from_v3_response(r: &[u8]) -> Result<Self> {
        let r: &[u8; 12] = r
            .get(..12)
            .and_then(|s| s.try_into().ok())
            .ok_or(Error::InvalidResponse("version (APIv3)"))?;
        Ok(Self {
            stlink: r[0],
            swim: r[1],
            jtag: r[2],
            msd: r[3],
            bridge: r[4],
            vid: u16::from_le_bytes([r[8], r[9]]),
            pid: u16::from_le_bytes([r[10], r[11]]),
            api: ApiVersion::V3,
        })
    }

    /// Features available in this firmware.
    pub fn capabilities(&self) -> ProbeCapabilities {
        match self.api {
            ApiVersion::V1 => ProbeCapabilities {
                api: ApiVersion::V1,
                target_voltage: false,
                get_last_rw_status2: false,
                swd_set_freq: false,
                mem_16bit: false,
                multi_ap: false,
                ap_init: false,
                max_rw8: 64,
            },
            // Version thresholds as documented in stlink/OpenOCD for V2 firmware.
            ApiVersion::V2 => ProbeCapabilities {
                api: ApiVersion::V2,
                target_voltage: self.jtag >= 13,
                get_last_rw_status2: self.jtag >= 15,
                swd_set_freq: self.jtag >= 22,
                mem_16bit: self.jtag >= 26,
                multi_ap: self.jtag >= 26,
                ap_init: self.jtag >= 28,
                max_rw8: 64,
            },
            ApiVersion::V3 => ProbeCapabilities {
                api: ApiVersion::V3,
                target_voltage: true,
                get_last_rw_status2: true,
                swd_set_freq: true,
                mem_16bit: true,
                multi_ap: true,
                ap_init: true,
                max_rw8: 512,
            },
        }
    }
}

/// `st-info` format: `V2J37S7`, `V3J7`, ...
impl fmt::Display for ProbeVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V{}", self.stlink)?;
        if self.jtag > 0 {
            write!(f, "J{}", self.jtag)?;
        }
        if self.swim > 0 {
            write!(f, "S{}", self.swim)?;
        }
        Ok(())
    }
}

/// What the probe firmware supports; commands outside this set are not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeCapabilities {
    pub api: ApiVersion,
    pub target_voltage: bool,
    pub get_last_rw_status2: bool,
    pub swd_set_freq: bool,
    pub mem_16bit: bool,
    /// Memory access through an AP other than 0.
    pub multi_ap: bool,
    /// The AP must be initialized (`INIT_AP`) before use.
    pub ap_init: bool,
    /// Max bytes per 8-bit memory transfer.
    pub max_rw8: usize,
}

impl ProbeCapabilities {
    /// Max bytes per 32-bit memory transfer (also the ADIv5 TAR auto-increment
    /// window, so chunks never cross a 1 KiB boundary).
    pub const MAX_RW32: usize = 1024;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_v2_clone() {
        let v = ProbeVersion::from_v1_response(&[0x29, 0x47, 0x83, 0x04, 0x48, 0x37]).ok();
        let v = v.as_ref();
        assert_eq!(v.map(|v| v.to_string()).as_deref(), Some("V2J37S7"));
        assert_eq!(
            v.map(|v| (v.vid, v.pid, v.api)),
            Some((0x0483, 0x3748, ApiVersion::V2))
        );
    }

    #[test]
    fn decode_v2_old_firmware_gating() {
        // V2J11S0
        let v = ProbeVersion::from_v1_response(&[0x22, 0xC0, 0x83, 0x04, 0x48, 0x37]);
        let caps = v.as_ref().map(|v| v.capabilities()).ok();
        assert_eq!(v.map(|v| v.to_string()).ok().as_deref(), Some("V2J11"));
        let caps = caps.as_ref();
        assert_eq!(caps.map(|c| c.target_voltage), Some(false));
        assert_eq!(caps.map(|c| c.get_last_rw_status2), Some(false));
    }

    #[test]
    fn decode_v3() {
        let r = [3, 0, 7, 3, 1, 0, 0, 0, 0x83, 0x04, 0x4F, 0x37];
        let v = ProbeVersion::from_v3_response(&r).ok();
        assert_eq!(v.map(|v| v.to_string()).as_deref(), Some("V3J7"));
        assert_eq!(v.map(|v| (v.msd, v.bridge, v.pid)), Some((3, 1, 0x374F)));
    }

    #[test]
    fn short_responses_are_errors() {
        assert!(ProbeVersion::from_v1_response(&[0x29, 0x47]).is_err());
        assert!(ProbeVersion::from_v3_response(&[3; 6]).is_err());
    }
}
