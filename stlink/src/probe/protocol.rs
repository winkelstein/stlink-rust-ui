//! ST-Link command codes and status values.
//!
//! Values cross-checked against stlink (BSD-3) and the OpenOCD ST-Link driver
//! (protocol knowledge only, no code taken).

use crate::{Error, Result};

pub(crate) const GET_VERSION: u8 = 0xF1;
pub(crate) const DEBUG_COMMAND: u8 = 0xF2;
pub(crate) const DFU_COMMAND: u8 = 0xF3;
pub(crate) const SWIM_COMMAND: u8 = 0xF4;
pub(crate) const GET_CURRENT_MODE: u8 = 0xF5;
pub(crate) const GET_TARGET_VOLTAGE: u8 = 0xF7;
pub(crate) const GET_VERSION_APIV3: u8 = 0xFB;

pub(crate) const DFU_EXIT: u8 = 0x07;
pub(crate) const SWIM_EXIT: u8 = 0x01;

/// Sub-commands of [`DEBUG_COMMAND`].
pub(crate) mod debug {
    pub(crate) const FORCEDEBUG: u8 = 0x02;
    pub(crate) const READMEM_32BIT: u8 = 0x07;
    pub(crate) const WRITEMEM_32BIT: u8 = 0x08;
    pub(crate) const RUNCORE: u8 = 0x09;
    pub(crate) const READMEM_8BIT: u8 = 0x0C;
    pub(crate) const WRITEMEM_8BIT: u8 = 0x0D;
    pub(crate) const EXIT: u8 = 0x21;
    pub(crate) const APIV2_ENTER: u8 = 0x30;
    pub(crate) const APIV2_READ_IDCODES: u8 = 0x31;
    pub(crate) const APIV2_RESETSYS: u8 = 0x32;
    pub(crate) const APIV2_READREG: u8 = 0x33;
    pub(crate) const APIV2_WRITE_DEBUG_REG: u8 = 0x35;
    pub(crate) const APIV2_READ_DEBUG_REG: u8 = 0x36;
    pub(crate) const APIV2_GETLASTRWSTATUS: u8 = 0x3B;
    pub(crate) const APIV2_DRIVE_NRST: u8 = 0x3C;
    pub(crate) const APIV2_GETLASTRWSTATUS2: u8 = 0x3E;
    pub(crate) const APIV2_SWD_SET_FREQ: u8 = 0x43;
    pub(crate) const APIV2_INIT_AP: u8 = 0x4B;
    pub(crate) const APIV3_SET_COM_FREQ: u8 = 0x61;
    pub(crate) const APIV3_GET_COM_FREQ: u8 = 0x62;

    /// Argument of `APIV2_ENTER`.
    pub(crate) const ENTER_SWD: u8 = 0xA3;
}

/// Mode reported by `GET_CURRENT_MODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    Dfu,
    Mass,
    Debug,
    Swim,
    Bootloader,
    Unknown(u8),
}

impl From<u8> for ProbeMode {
    fn from(v: u8) -> Self {
        match v {
            0x00 => ProbeMode::Dfu,
            0x01 => ProbeMode::Mass,
            0x02 => ProbeMode::Debug,
            0x03 => ProbeMode::Swim,
            0x04 => ProbeMode::Bootloader,
            other => ProbeMode::Unknown(other),
        }
    }
}

pub(crate) const STATUS_OK: u8 = 0x80;
const STATUS_FAULT: u8 = 0x81;
const SWD_AP_WAIT: u8 = 0x10;
const SWD_DP_WAIT: u8 = 0x14;
const JTAG_GET_IDCODE_ERROR: u8 = 0x09;

/// Human-readable name of an ST-Link status byte.
pub fn status_name(status: u8) -> &'static str {
    match status {
        STATUS_OK => "OK",
        STATUS_FAULT => "fault",
        JTAG_GET_IDCODE_ERROR => "cannot read IDCODE",
        SWD_AP_WAIT => "AP wait",
        0x11 => "AP fault",
        0x12 => "AP error",
        0x13 => "AP parity error",
        SWD_DP_WAIT => "DP wait",
        0x15 => "DP fault",
        0x16 => "DP error",
        0x17 => "DP parity error",
        0x18 => "AP write data error",
        0x19 => "AP sticky error",
        0x1A => "AP sticky overrun",
        0x1D => "bad AP",
        _ => "unknown status",
    }
}

/// Map a status byte to `Ok(())` or a typed error.
pub(crate) fn check_status(status: u8) -> Result<()> {
    match status {
        STATUS_OK => Ok(()),
        SWD_AP_WAIT => Err(Error::ApWait),
        SWD_DP_WAIT => Err(Error::DpWait),
        JTAG_GET_IDCODE_ERROR => Err(Error::TargetNotConnected),
        STATUS_FAULT | 0x11..=0x13 | 0x15..=0x1A | 0x1D => Err(Error::SwdFault(status)),
        other => Err(Error::ProbeStatus(other)),
    }
}

/// Little-endian u32 at `offset`, or an invalid-response error.
pub(crate) fn le_u32(data: &[u8], offset: usize) -> Result<u32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(Error::InvalidResponse("response too short"))
}

/// Target voltage from the two ADC readings of `GET_TARGET_VOLTAGE`.
pub(crate) fn target_voltage(adc0: u32, adc1: u32) -> Option<f32> {
    (adc0 != 0).then(|| 2.0 * adc1 as f32 * 1.2 / adc0 as f32)
}

/// V2 SWD clock divisors: `(kHz, divisor)`, fastest first.
const V2_SWD_FREQ: &[(u32, u16)] = &[
    (4000, 0),
    (1800, 1),
    (1200, 2),
    (950, 3),
    (480, 7),
    (240, 15),
    (125, 31),
    (100, 40),
    (50, 79),
    (25, 158),
    (15, 265),
    (5, 798),
];

/// Fastest V2 setting not above `khz` (the slowest one if `khz` is below all).
pub(crate) fn v2_swd_divisor(khz: u32) -> (u32, u16) {
    V2_SWD_FREQ
        .iter()
        .copied()
        .find(|&(f, _)| f <= khz)
        .unwrap_or((5, 798))
}

/// Fastest frequency from a V3 list not above `khz` (the slowest if none).
pub(crate) fn pick_v3_freq(available: &[u32], khz: u32) -> Option<u32> {
    available
        .iter()
        .copied()
        .filter(|&f| f <= khz)
        .max()
        .or_else(|| available.iter().copied().min())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        assert!(check_status(0x80).is_ok());
        assert!(matches!(check_status(0x10), Err(Error::ApWait)));
        assert!(matches!(check_status(0x14), Err(Error::DpWait)));
        assert!(matches!(check_status(0x11), Err(Error::SwdFault(0x11))));
        assert!(matches!(check_status(0x81), Err(Error::SwdFault(0x81))));
        assert!(matches!(check_status(0x09), Err(Error::TargetNotConnected)));
        assert!(matches!(check_status(0x42), Err(Error::ProbeStatus(0x42))));
    }

    #[test]
    fn mode_decoding() {
        assert_eq!(ProbeMode::from(0), ProbeMode::Dfu);
        assert_eq!(ProbeMode::from(2), ProbeMode::Debug);
        assert_eq!(ProbeMode::from(9), ProbeMode::Unknown(9));
    }

    #[test]
    fn le_u32_bounds() {
        assert_eq!(
            le_u32(&[0x80, 0, 0, 0, 0x71, 0xC2, 0x1F, 0x41], 4).ok(),
            Some(0x411F_C271)
        );
        assert!(le_u32(&[1, 2, 3], 0).is_err());
    }

    #[test]
    fn swd_divisors() {
        assert_eq!(v2_swd_divisor(4000), (4000, 0));
        assert_eq!(v2_swd_divisor(10_000), (4000, 0));
        assert_eq!(v2_swd_divisor(1800), (1800, 1));
        assert_eq!(v2_swd_divisor(1000), (950, 3));
        assert_eq!(v2_swd_divisor(1), (5, 798));
    }

    #[test]
    fn v3_freq_choice() {
        let list = [24000, 8000, 3300, 1000, 200, 50, 5];
        assert_eq!(pick_v3_freq(&list, 4000), Some(3300));
        assert_eq!(pick_v3_freq(&list, 100_000), Some(24000));
        assert_eq!(pick_v3_freq(&list, 1), Some(5));
        assert_eq!(pick_v3_freq(&[], 1000), None);
    }

    #[test]
    fn voltage_formula() {
        // adc0 = internal 1.2 V reference, adc1 = half of target voltage.
        let v = target_voltage(1500, 2063).unwrap_or_default();
        assert!((v - 3.30).abs() < 0.01, "{v}");
        assert_eq!(target_voltage(0, 100), None);
    }
}
