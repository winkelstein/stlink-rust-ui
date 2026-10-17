//! Library error type.
//!
//! Every device-dependent operation returns [`Result`]; the library never panics
//! on device input (release builds use `panic = "abort"`).

use nusb::transfer::TransferError;

/// Library result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors returned by the `stlink` library.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// No matching device found.
    #[error("device not found")]
    NotFound,

    /// The OS denied access to the device.
    #[error("access to the USB device denied")]
    AccessDenied,

    /// The device or its interface is claimed by another program (openocd, st-util, ...).
    #[error("USB device is busy (used by another program?)")]
    Busy,

    /// The device was unplugged during the operation.
    #[error("device disconnected")]
    Disconnected,

    /// A USB operation (or a wait for the target) did not complete within its timeout.
    #[error("operation timed out")]
    Timeout,

    /// Other USB error while opening/configuring the device.
    #[error("USB error: {0}")]
    Usb(nusb::Error),

    /// Other USB transfer error.
    #[error("USB transfer error: {0}")]
    Transfer(TransferError),

    /// The probe answered with something that does not match the protocol.
    #[error("invalid response from probe: {0}")]
    InvalidResponse(&'static str),

    /// The caller passed arguments the operation cannot handle.
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),

    /// ST-Link returned a status other than OK that has no dedicated variant.
    #[error("ST-Link status 0x{0:02X}")]
    ProbeStatus(u8),

    /// AP returned WAIT (retries exhausted).
    #[error("SWD AP wait")]
    ApWait,

    /// DP returned WAIT (retries exhausted).
    #[error("SWD DP wait")]
    DpWait,

    /// SWD transaction failed (fault/error/parity).
    #[error("SWD fault: {name} (status 0x{0:02X})", name = crate::probe::status_name(*.0))]
    SwdFault(u8),

    /// The probe could not connect to the target.
    #[error("target not connected (check wiring and target power, or try connect under reset)")]
    TargetNotConnected,

    /// The chip answers on a debug AP, but the core's AP0 does not respond
    /// (typically low-power firmware with debug clocks off).
    #[error(
        "{0}: core not accessible (firmware in low-power mode?); enable debug in low-power modes or connect under reset"
    )]
    CoreNotAccessible(String),

    /// DBGMCU_IDCODE value not present in the chip table.
    #[error("unknown chip (IDCODE 0x{0:08X})")]
    UnknownChip(u32),

    /// Operation not supported by this probe firmware or target.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    /// The operation was cancelled by the caller.
    #[error("cancelled")]
    Cancelled,

    /// Flash is read/write protected.
    #[error("flash is protected")]
    FlashProtected,

    /// The flash controller reported an error (status register value).
    #[error("flash operation failed (status 0x{status:08X})")]
    FlashOperation { status: u32 },

    /// Read-back verification failed.
    #[error("verification failed at 0x{addr:08X}")]
    Verify { addr: u32 },
}

impl From<nusb::Error> for Error {
    fn from(err: nusb::Error) -> Self {
        match err.kind() {
            nusb::ErrorKind::Disconnected => Error::Disconnected,
            nusb::ErrorKind::PermissionDenied => Error::AccessDenied,
            nusb::ErrorKind::Busy => Error::Busy,
            _ => Error::Usb(err),
        }
    }
}

impl From<TransferError> for Error {
    fn from(err: TransferError) -> Self {
        match err {
            TransferError::Disconnected => Error::Disconnected,
            other => Error::Transfer(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_disconnect_maps_to_disconnected() {
        assert!(matches!(
            Error::from(TransferError::Disconnected),
            Error::Disconnected
        ));
    }

    #[test]
    fn other_transfer_errors_are_kept() {
        assert!(matches!(
            Error::from(TransferError::Stall),
            Error::Transfer(TransferError::Stall)
        ));
    }

    #[test]
    fn display_formats_hex() {
        assert_eq!(Error::ProbeStatus(0x81).to_string(), "ST-Link status 0x81");
        assert_eq!(
            Error::UnknownChip(0x1000_6450).to_string(),
            "unknown chip (IDCODE 0x10006450)"
        );
        assert_eq!(
            Error::SwdFault(0x11).to_string(),
            "SWD fault: AP fault (status 0x11)"
        );
    }
}
