//! ST-Link protocol layer: one open probe and its commands.

mod protocol;
mod version;

use std::time::Duration;

use tracing::{debug, info, warn};

use crate::usb::ProbeDescriptor;
use crate::usb::transport::Transport;
use crate::{Error, Result};
pub(crate) use protocol::le_u32;
use protocol::{DEBUG_COMMAND, debug as dbg};

pub use protocol::{ProbeMode, status_name};
pub use version::{ApiVersion, ProbeCapabilities, ProbeVersion};

/// Default timeout of a single USB transfer.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1000);

/// How to drive the NRST line (`DRIVE_NRST`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NrstAction {
    Low = 0,
    High = 1,
    Pulse = 2,
}

/// An opened ST-Link. Commands are serialized through `&mut self`.
///
/// On drop, leaves SWD debug mode (best effort).
pub struct StLink {
    transport: Transport,
    descriptor: ProbeDescriptor,
    version: ProbeVersion,
    caps: ProbeCapabilities,
    in_debug: bool,
}

impl StLink {
    /// Open the probe with the default timeout and bring it out of DFU/debug/SWIM mode.
    pub fn open(descriptor: &ProbeDescriptor) -> Result<Self> {
        Self::open_with_timeout(descriptor, DEFAULT_TIMEOUT)
    }

    pub fn open_with_timeout(descriptor: &ProbeDescriptor, timeout: Duration) -> Result<Self> {
        let mut transport = Transport::open(descriptor, timeout)?;
        let version = read_version(&mut transport)?;
        let caps = version.capabilities();
        info!(%version, ?caps, "opened {} {}", descriptor.model, descriptor.serial);
        if version.api == ApiVersion::V1 {
            return Err(Error::Unsupported("ST-Link V1 (debug API v1)"));
        }
        let mut probe = Self {
            transport,
            descriptor: descriptor.clone(),
            version,
            caps,
            in_debug: false,
        };
        probe.leave_current_mode()?;
        Ok(probe)
    }

    pub fn descriptor(&self) -> &ProbeDescriptor {
        &self.descriptor
    }

    pub fn version(&self) -> &ProbeVersion {
        &self.version
    }

    pub fn capabilities(&self) -> &ProbeCapabilities {
        &self.caps
    }

    /// Whether SWD debug mode has been entered.
    pub fn in_debug_mode(&self) -> bool {
        self.in_debug
    }

    pub fn current_mode(&mut self) -> Result<ProbeMode> {
        let r = self
            .transport
            .command("GET_CURRENT_MODE", &[protocol::GET_CURRENT_MODE], 2)?;
        Ok(ProbeMode::from(r[0]))
    }

    fn leave_current_mode(&mut self) -> Result<()> {
        let mode = self.current_mode()?;
        debug!(?mode, "current probe mode");
        match mode {
            ProbeMode::Dfu => {
                self.transport.command(
                    "DFU_EXIT",
                    &[protocol::DFU_COMMAND, protocol::DFU_EXIT],
                    0,
                )?;
            }
            ProbeMode::Debug => {
                self.transport
                    .command("EXIT", &[DEBUG_COMMAND, dbg::EXIT], 0)?;
            }
            ProbeMode::Swim => {
                self.transport.command(
                    "SWIM_EXIT",
                    &[protocol::SWIM_COMMAND, protocol::SWIM_EXIT],
                    0,
                )?;
            }
            ProbeMode::Mass | ProbeMode::Bootloader | ProbeMode::Unknown(_) => {}
        }
        Ok(())
    }

    /// Target voltage in volts; `None` if the firmware cannot measure it.
    pub fn target_voltage(&mut self) -> Result<Option<f32>> {
        if !self.caps.target_voltage {
            return Ok(None);
        }
        let r = self
            .transport
            .command("GET_TARGET_VOLTAGE", &[protocol::GET_TARGET_VOLTAGE], 8)?;
        let v = protocol::target_voltage(le_u32(&r, 0)?, le_u32(&r, 4)?);
        if v.is_none() {
            warn!("target voltage: ADC reference reads 0");
        }
        Ok(v)
    }

    /// Set the SWD clock; returns the frequency actually used, kHz.
    ///
    /// Call before [`StLink::enter_swd`]. Firmware without this command keeps
    /// its default (1.8 MHz on V2).
    pub fn set_swd_frequency(&mut self, khz: u32) -> Result<u32> {
        if !self.caps.swd_set_freq {
            return Err(Error::Unsupported(
                "setting SWD frequency (firmware too old)",
            ));
        }
        let actual = match self.caps.api {
            ApiVersion::V3 => {
                let r = self.transport.command(
                    "GET_COM_FREQ",
                    &[DEBUG_COMMAND, dbg::APIV3_GET_COM_FREQ, 0],
                    52,
                )?;
                protocol::check_status(r[0])?;
                let count = usize::from(r[8]).min(10);
                let list = (0..count)
                    .map(|i| le_u32(&r, 12 + 4 * i))
                    .collect::<Result<Vec<_>>>()?;
                let khz = protocol::pick_v3_freq(&list, khz)
                    .ok_or(Error::InvalidResponse("empty SWD frequency list"))?;
                let mut cmd = [DEBUG_COMMAND, dbg::APIV3_SET_COM_FREQ, 0, 0, 0, 0, 0, 0];
                cmd[4..8].copy_from_slice(&khz.to_le_bytes());
                let r = self.transport.command("SET_COM_FREQ", &cmd, 8)?;
                protocol::check_status(r[0])?;
                khz
            }
            _ => {
                let (khz, div) = protocol::v2_swd_divisor(khz);
                let [lo, hi] = div.to_le_bytes();
                self.status_cmd(
                    "SWD_SET_FREQ",
                    &[DEBUG_COMMAND, dbg::APIV2_SWD_SET_FREQ, lo, hi],
                )?;
                khz
            }
        };
        info!("SWD clock {actual} kHz");
        Ok(actual)
    }

    /// Enter SWD debug mode (hot plug: the target is not reset).
    pub fn enter_swd(&mut self) -> Result<()> {
        self.status_cmd(
            "ENTER_SWD",
            &[DEBUG_COMMAND, dbg::APIV2_ENTER, dbg::ENTER_SWD],
        )?;
        self.in_debug = true;
        match self.read_dp_idcode() {
            Ok(id) => debug!("SW-DP IDCODE 0x{id:08X}"),
            Err(e) => debug!("SW-DP IDCODE: {e}"),
        }
        Ok(())
    }

    /// Leave debug mode; the target keeps its current run state.
    pub fn exit_debug(&mut self) -> Result<()> {
        self.transport
            .command("EXIT", &[DEBUG_COMMAND, dbg::EXIT], 0)?;
        self.in_debug = false;
        Ok(())
    }

    /// SW-DP IDCODE.
    pub fn read_dp_idcode(&mut self) -> Result<u32> {
        let r = self.transport.command(
            "READ_IDCODES",
            &[DEBUG_COMMAND, dbg::APIV2_READ_IDCODES],
            12,
        )?;
        protocol::check_status(r[0])?;
        le_u32(&r, 4)
    }

    /// Read one 32-bit word with `READ_DEBUG_REG` (address must be 4-aligned).
    ///
    /// Prefer [`StLink::read_u32`]: V2 firmware (seen on J46) reports OK with
    /// stale data here when the AP access fails.
    pub fn read_debug32(&mut self, addr: u32) -> Result<u32> {
        check_align(addr, 4)?;
        let mut cmd = [DEBUG_COMMAND, dbg::APIV2_READ_DEBUG_REG, 0, 0, 0, 0];
        cmd[2..6].copy_from_slice(&addr.to_le_bytes());
        let r = self.transport.command("READ_DEBUG_REG", &cmd, 8)?;
        protocol::check_status(r[0])?;
        le_u32(&r, 4)
    }

    /// Write one 32-bit word with `WRITE_DEBUG_REG` (address must be 4-aligned).
    /// Prefer [`StLink::write_u32`], which reports AP failures.
    pub fn write_debug32(&mut self, addr: u32, value: u32) -> Result<()> {
        check_align(addr, 4)?;
        let mut cmd = [0u8; 10];
        cmd[..2].copy_from_slice(&[DEBUG_COMMAND, dbg::APIV2_WRITE_DEBUG_REG]);
        cmd[2..6].copy_from_slice(&addr.to_le_bytes());
        cmd[6..10].copy_from_slice(&value.to_le_bytes());
        self.status_cmd("WRITE_DEBUG_REG", &cmd)
    }

    /// One 32-bit read transfer: `addr` and `len` 4-aligned, `len ≤ MAX_RW32`.
    pub fn read_mem32(&mut self, addr: u32, len: usize) -> Result<Vec<u8>> {
        check_align(addr, 4)?;
        if len == 0 || len % 4 != 0 || len > ProbeCapabilities::MAX_RW32 {
            return Err(Error::InvalidArgument("32-bit read length"));
        }
        let data = self.transport.command(
            "READMEM_32BIT",
            &mem_cmd(dbg::READMEM_32BIT, addr, len),
            len,
        )?;
        self.last_rw_status()?;
        Ok(data)
    }

    /// Prepare an AP for memory access (needed for AP ≠ 0 on newer firmware).
    pub fn init_ap(&mut self, ap: u8) -> Result<()> {
        if !self.caps.multi_ap {
            return Err(Error::Unsupported(
                "access to AP other than 0 (firmware too old)",
            ));
        }
        if self.caps.ap_init {
            self.status_cmd("INIT_AP", &[DEBUG_COMMAND, dbg::APIV2_INIT_AP, ap, 0])?;
        }
        Ok(())
    }

    /// One 32-bit read through AP `ap` (call [`StLink::init_ap`] first).
    pub fn read_mem32_ap(&mut self, ap: u8, addr: u32, len: usize) -> Result<Vec<u8>> {
        if ap == 0 {
            return self.read_mem32(addr, len);
        }
        if !self.caps.multi_ap {
            return Err(Error::Unsupported(
                "access to AP other than 0 (firmware too old)",
            ));
        }
        check_align(addr, 4)?;
        if len == 0 || len % 4 != 0 || len > ProbeCapabilities::MAX_RW32 {
            return Err(Error::InvalidArgument("32-bit read length"));
        }
        let data = self.transport.command(
            "READMEM_32BIT",
            &mem_cmd_ap(dbg::READMEM_32BIT, addr, len, ap),
            len,
        )?;
        self.last_rw_status()?;
        Ok(data)
    }

    /// One 32-bit write through AP `ap` (call [`StLink::init_ap`] first).
    pub fn write_mem32_ap(&mut self, ap: u8, addr: u32, data: &[u8]) -> Result<()> {
        if ap == 0 {
            return self.write_mem32(addr, data);
        }
        if !self.caps.multi_ap {
            return Err(Error::Unsupported(
                "access to AP other than 0 (firmware too old)",
            ));
        }
        check_align(addr, 4)?;
        if data.is_empty() || data.len() % 4 != 0 || data.len() > ProbeCapabilities::MAX_RW32 {
            return Err(Error::InvalidArgument("32-bit write length"));
        }
        self.transport.command(
            "WRITEMEM_32BIT",
            &mem_cmd_ap(dbg::WRITEMEM_32BIT, addr, data.len(), ap),
            0,
        )?;
        self.transport.write_data("WRITEMEM_32BIT", data)?;
        self.last_rw_status()
    }

    /// One 32-bit write transfer: `addr` and `data.len()` 4-aligned, `len ≤ MAX_RW32`.
    pub fn write_mem32(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        check_align(addr, 4)?;
        if data.is_empty() || data.len() % 4 != 0 || data.len() > ProbeCapabilities::MAX_RW32 {
            return Err(Error::InvalidArgument("32-bit write length"));
        }
        self.transport.command(
            "WRITEMEM_32BIT",
            &mem_cmd(dbg::WRITEMEM_32BIT, addr, data.len()),
            0,
        )?;
        self.transport.write_data("WRITEMEM_32BIT", data)?;
        self.last_rw_status()
    }

    /// One 8-bit read transfer, `len ≤ max_rw8`.
    pub fn read_mem8(&mut self, addr: u32, len: usize) -> Result<Vec<u8>> {
        if len == 0 || len > self.caps.max_rw8 {
            return Err(Error::InvalidArgument("8-bit read length"));
        }
        // A 1-byte read returns 2 bytes.
        let resp_len = len.max(2);
        let mut data = self.transport.command(
            "READMEM_8BIT",
            &mem_cmd(dbg::READMEM_8BIT, addr, len),
            resp_len,
        )?;
        data.truncate(len);
        self.last_rw_status()?;
        Ok(data)
    }

    /// One 8-bit write transfer, `len ≤ max_rw8`.
    pub fn write_mem8(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        if data.is_empty() || data.len() > self.caps.max_rw8 {
            return Err(Error::InvalidArgument("8-bit write length"));
        }
        self.transport.command(
            "WRITEMEM_8BIT",
            &mem_cmd(dbg::WRITEMEM_8BIT, addr, data.len()),
            0,
        )?;
        self.transport.write_data("WRITEMEM_8BIT", data)?;
        self.last_rw_status()
    }

    /// Core register by `READREG` index (0–15 = R0–PC, 16 = xPSR, 17 = MSP, 18 = PSP).
    /// The core must be halted.
    pub fn read_core_reg(&mut self, index: u8) -> Result<u32> {
        let r =
            self.transport
                .command("READREG", &[DEBUG_COMMAND, dbg::APIV2_READREG, index], 8)?;
        protocol::check_status(r[0])?;
        le_u32(&r, 4)
    }

    pub fn force_debug(&mut self) -> Result<()> {
        self.status_cmd("FORCEDEBUG", &[DEBUG_COMMAND, dbg::FORCEDEBUG])
    }

    pub fn run_core(&mut self) -> Result<()> {
        self.status_cmd("RUNCORE", &[DEBUG_COMMAND, dbg::RUNCORE])
    }

    /// System reset through the probe (AIRCR SYSRESETREQ).
    pub fn reset_sys(&mut self) -> Result<()> {
        self.status_cmd("RESETSYS", &[DEBUG_COMMAND, dbg::APIV2_RESETSYS])
    }

    /// Drive the NRST pin (needs NRST wired to the probe).
    pub fn drive_nrst(&mut self, action: NrstAction) -> Result<()> {
        self.status_cmd(
            "DRIVE_NRST",
            &[DEBUG_COMMAND, dbg::APIV2_DRIVE_NRST, action as u8],
        )
    }

    /// Status of the last memory read/write.
    fn last_rw_status(&mut self) -> Result<()> {
        if self.caps.get_last_rw_status2 {
            let r = self.transport.command(
                "GETLASTRWSTATUS2",
                &[DEBUG_COMMAND, dbg::APIV2_GETLASTRWSTATUS2],
                12,
            )?;
            protocol::check_status(r[0]).inspect_err(|e| {
                debug!(
                    "last R/W failed at 0x{:08X}: {e}",
                    le_u32(&r, 4).unwrap_or_default()
                );
            })
        } else {
            let r = self.transport.command(
                "GETLASTRWSTATUS",
                &[DEBUG_COMMAND, dbg::APIV2_GETLASTRWSTATUS],
                2,
            )?;
            protocol::check_status(r[0])
        }
    }

    /// Command whose 2-byte response starts with a status byte.
    fn status_cmd(&mut self, name: &str, cmd: &[u8]) -> Result<()> {
        let r = self.transport.command(name, cmd, 2)?;
        protocol::check_status(r[0])
    }
}

impl Drop for StLink {
    fn drop(&mut self) {
        if self.in_debug
            && let Err(e) = self.exit_debug()
        {
            debug!("exit debug mode on drop: {e}");
        }
    }
}

fn read_version(t: &mut Transport) -> Result<ProbeVersion> {
    let r = t.command("GET_VERSION", &[protocol::GET_VERSION], 6)?;
    let v = ProbeVersion::from_v1_response(&r)?;
    if v.stlink < 3 {
        return Ok(v);
    }
    let r = t.command("GET_VERSION_APIV3", &[protocol::GET_VERSION_APIV3], 12)?;
    ProbeVersion::from_v3_response(&r)
}

fn mem_cmd(sub: u8, addr: u32, len: usize) -> [u8; 8] {
    let mut cmd = [DEBUG_COMMAND, sub, 0, 0, 0, 0, 0, 0];
    cmd[2..6].copy_from_slice(&addr.to_le_bytes());
    // Callers bound `len` well below u16::MAX.
    cmd[6..8].copy_from_slice(&(len as u16).to_le_bytes());
    cmd
}

/// Memory command with an AP number (multi-AP firmware); CSW left 0 = default.
fn mem_cmd_ap(sub: u8, addr: u32, len: usize, ap: u8) -> [u8; 9] {
    let mut cmd = [0u8; 9];
    cmd[..8].copy_from_slice(&mem_cmd(sub, addr, len));
    cmd[8] = ap;
    cmd
}

fn check_align(addr: u32, align: u32) -> Result<()> {
    if addr % align == 0 {
        Ok(())
    } else {
        Err(Error::InvalidArgument("unaligned address"))
    }
}
