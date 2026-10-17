//! `stlink::Error` → `{ kind, message }` for the frontend (`BackendError`).

use serde::Serialize;

/// Error kinds the UI treats specially; everything else is `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorKind {
    Disconnected,
    CoreNotAccessible,
    FlashProtected,
    Busy,
    /// Windows: the device is not on WinUSB.
    DriverMissing,
    Unsupported,
    Cancelled,
    Other,
}

/// What every command rejects with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendError {
    pub kind: ErrorKind,
    pub message: String,
}

pub type Result<T, E = BackendError> = std::result::Result<T, E>;

impl BackendError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Other, message)
    }

    /// The probe is gone (unplugged, or its worker thread stopped).
    pub fn disconnected() -> Self {
        stlink::Error::Disconnected.into()
    }
}

impl From<stlink::Error> for BackendError {
    fn from(e: stlink::Error) -> Self {
        use stlink::Error as E;
        let kind = match e {
            E::Disconnected => ErrorKind::Disconnected,
            E::CoreNotAccessible(_) => ErrorKind::CoreNotAccessible,
            E::FlashProtected => ErrorKind::FlashProtected,
            E::Busy => ErrorKind::Busy,
            E::DriverMissing { .. } => ErrorKind::DriverMissing,
            E::Unsupported(_) => ErrorKind::Unsupported,
            E::Cancelled => ErrorKind::Cancelled,
            _ => ErrorKind::Other,
        };
        Self::new(kind, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_special_kinds() {
        let cases = [
            (stlink::Error::Disconnected, ErrorKind::Disconnected),
            (
                stlink::Error::CoreNotAccessible("STM32H74x_H75x".into()),
                ErrorKind::CoreNotAccessible,
            ),
            (stlink::Error::FlashProtected, ErrorKind::FlashProtected),
            (stlink::Error::Busy, ErrorKind::Busy),
            (
                stlink::Error::DriverMissing {
                    driver: "STLinkUSB".into(),
                },
                ErrorKind::DriverMissing,
            ),
            (stlink::Error::Unsupported("x"), ErrorKind::Unsupported),
            (stlink::Error::Cancelled, ErrorKind::Cancelled),
            (stlink::Error::Timeout, ErrorKind::Other),
            (stlink::Error::Verify { addr: 0x0800_0000 }, ErrorKind::Other),
        ];
        for (e, kind) in cases {
            assert_eq!(BackendError::from(e).kind, kind);
        }
    }

    #[test]
    fn keeps_display_text_and_serializes() {
        let e = BackendError::from(stlink::Error::Busy);
        assert_eq!(e.message, "USB device is busy (used by another program?)");
        let json = serde_json::to_value(&e).unwrap_or_default();
        assert_eq!(json["kind"], "Busy");
        assert_eq!(json["message"], e.message);
    }
}
