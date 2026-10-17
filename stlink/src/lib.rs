//! Access to STM32 microcontrollers via ST-Link probes (V2, V2-1, V3) and
//! the STM32 system bootloader over USB DFU (DfuSe).
//!
//! USB goes through [`nusb`]; diagnostics go through [`tracing`] (packet hex
//! dumps at `trace` level).

pub mod arm;
pub mod chips;
pub mod error;
pub mod flash;
pub mod probe;
pub mod target;
pub mod usb;

pub use arm::{CoreInfo, CoreRegisters, CoreType};
pub use chips::ChipDescriptor;
pub use error::{Error, Result};
pub use probe::{ProbeCapabilities, ProbeVersion, StLink};
pub use target::TargetInfo;
pub use usb::{ProbeDescriptor, ProbeModel, list_probes};
