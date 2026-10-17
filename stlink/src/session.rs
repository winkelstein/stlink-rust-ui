//! Connecting to a target: the sequence both the CLI and the GUI use.

use tracing::{debug, info};

use crate::probe::StLink;
use crate::{Error, Result};

/// Default SWD clock, kHz (same as st-flash and OpenOCD).
pub const DEFAULT_SWD_KHZ: u32 = 1800;

/// How to connect to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectOptions {
    /// SWD clock, kHz; rounded down to what the probe supports. Always set
    /// explicitly: the probe otherwise keeps the clock of the previous session.
    pub swd_khz: u32,
    /// Reset the target while attaching (NRST, falling back to AIRCR.SYSRESETREQ).
    /// Needed when the firmware disables SWD or sleeps with debug clocks off.
    pub under_reset: bool,
    /// Set DBGMCU_CR so debug keeps working in Sleep/Stop/Standby (until
    /// power-off). On H7 this goes through AP2 and works while the core sleeps.
    pub keep_debug_in_low_power: bool,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            swd_khz: DEFAULT_SWD_KHZ,
            under_reset: false,
            keep_debug_in_low_power: false,
        }
    }
}

/// What [`StLink::connect`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connection {
    /// SWD clock in use; `None` if the firmware cannot set it.
    pub swd_khz: Option<u32>,
    /// The core was halted by the connect sequence (under reset); call
    /// [`StLink::release`] to let it run again.
    pub halted_by_connect: bool,
    /// Result of `keep_debug_in_low_power`: `None` if not requested,
    /// `Some(false)` if the chip family has no known DBGMCU layout.
    pub low_power_debug: Option<bool>,
}

impl StLink {
    /// Set the SWD clock, enter SWD (optionally under reset) and apply the
    /// DBGMCU option. Afterwards the target is ready for [`StLink::identify`].
    pub fn connect(&mut self, opts: &ConnectOptions) -> Result<Connection> {
        let swd_khz = match self.set_swd_frequency(opts.swd_khz) {
            Ok(khz) => Some(khz),
            // Old firmware keeps its fixed default clock.
            Err(e @ Error::Unsupported(_)) => {
                debug!("{e}");
                None
            }
            Err(e) => return Err(e),
        };

        // H7: DBGMCU is reachable through AP2 even while the core sleeps, so
        // enable debug clocks before touching the core.
        let mut low_power_debug = None;
        if opts.keep_debug_in_low_power {
            self.enter_swd()?;
            if let Some(chip) = self.identify_via_debug_ap()?
                && chip.dbgmcu_low_power().is_some_and(|d| d.ap != 0)
            {
                low_power_debug = Some(self.enable_debug_in_low_power(chip)?);
            }
        }

        if opts.under_reset {
            self.attach_under_reset()?;
        } else if !self.in_debug_mode() {
            self.enter_swd()?;
        }

        // Families whose DBGMCU sits behind AP0 need the core to be reachable.
        if opts.keep_debug_in_low_power && low_power_debug.is_none() {
            let target = self.identify()?;
            low_power_debug = Some(match target.chip {
                Some(chip) => self.enable_debug_in_low_power(chip)?,
                None => false,
            });
        }

        info!(
            ?swd_khz,
            under_reset = opts.under_reset,
            ?low_power_debug,
            "connected"
        );
        Ok(Connection {
            swd_khz,
            halted_by_connect: opts.under_reset,
            low_power_debug,
        })
    }

    /// Undo the run-state change of [`StLink::connect`]: resume the core if
    /// the connect sequence halted it.
    pub fn release(&mut self, connection: &Connection) -> Result<()> {
        if connection.halted_by_connect {
            self.run()?;
        }
        Ok(())
    }
}
