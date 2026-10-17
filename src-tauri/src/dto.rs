//! Serializable mirrors of the `stlink` types: the shapes of
//! `src/backend/types.ts` (camelCase).

use serde::{Deserialize, Serialize};
use stlink::flash::{FlashGeometry, RdpLevel, Sector, WritePlan};
use stlink::probe::ApiVersion;
use stlink::{CoreType, ProbeDescriptor, ProbeModel};

/// Stable id of a connected probe: `bus_id:device_address`.
pub fn probe_id(d: &ProbeDescriptor) -> String {
    format!("{}:{}", d.bus_id, d.device_address)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeDescriptorDto {
    pub vid: u16,
    pub pid: u16,
    pub model: &'static str,
    pub serial: String,
    pub bus_id: String,
    pub device_address: u8,
}

impl From<&ProbeDescriptor> for ProbeDescriptorDto {
    fn from(d: &ProbeDescriptor) -> Self {
        Self {
            vid: d.vid,
            pid: d.pid,
            model: match d.model {
                ProbeModel::V2 => "V2",
                ProbeModel::V2_1 => "V2_1",
                ProbeModel::V3 => "V3",
            },
            serial: d.serial.clone(),
            bus_id: d.bus_id.clone(),
            device_address: d.device_address,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeVersionDto {
    pub stlink: u8,
    pub jtag: u8,
    pub swim: u8,
    pub msd: u8,
    pub bridge: u8,
    pub api: &'static str,
    pub display: String,
}

impl From<&stlink::ProbeVersion> for ProbeVersionDto {
    fn from(v: &stlink::ProbeVersion) -> Self {
        Self {
            stlink: v.stlink,
            jtag: v.jtag,
            swim: v.swim,
            msd: v.msd,
            bridge: v.bridge,
            api: match v.api {
                ApiVersion::V1 => "V1",
                ApiVersion::V2 => "V2",
                ApiVersion::V3 => "V3",
            },
            display: v.to_string(),
        }
    }
}

/// A discovered probe, from the USB descriptor only (the device is not opened).
#[derive(Debug, Clone, Serialize)]
pub struct ProbeSummary {
    pub id: String,
    pub descriptor: ProbeDescriptorDto,
}

impl From<&ProbeDescriptor> for ProbeSummary {
    fn from(d: &ProbeDescriptor) -> Self {
        Self {
            id: probe_id(d),
            descriptor: d.into(),
        }
    }
}

/// An opened probe. `version` is `None` until the first successful open.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub id: String,
    pub descriptor: ProbeDescriptorDto,
    pub version: Option<ProbeVersionDto>,
    pub target_voltage: Option<f32>,
}

/// `session::ConnectOptions` as sent by the frontend.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOptionsDto {
    pub swd_khz: u32,
    pub under_reset: bool,
    pub keep_debug_in_low_power: bool,
}

impl From<ConnectOptionsDto> for stlink::ConnectOptions {
    fn from(o: ConnectOptionsDto) -> Self {
        Self {
            swd_khz: o.swd_khz,
            under_reset: o.under_reset,
            keep_debug_in_low_power: o.keep_debug_in_low_power,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDto {
    pub swd_khz: Option<u32>,
    pub halted_by_connect: bool,
    pub low_power_debug: Option<bool>,
}

impl From<&stlink::Connection> for ConnectionDto {
    fn from(c: &stlink::Connection) -> Self {
        Self {
            swd_khz: c.swd_khz,
            halted_by_connect: c.halted_by_connect,
            low_power_debug: c.low_power_debug,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoreInfoDto {
    pub cpuid: u32,
    pub implementer: u8,
    pub core: &'static str,
    pub revision: (u8, u8),
}

impl From<&stlink::CoreInfo> for CoreInfoDto {
    fn from(c: &stlink::CoreInfo) -> Self {
        Self {
            cpuid: c.cpuid,
            implementer: c.implementer,
            core: match c.core {
                CoreType::M0 => "M0",
                CoreType::M0Plus => "M0Plus",
                CoreType::M3 => "M3",
                CoreType::M4 => "M4",
                CoreType::M7 => "M7",
                CoreType::M23 => "M23",
                CoreType::M33 => "M33",
                CoreType::Unknown(_) => "Unknown",
            },
            revision: c.revision,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChipInfo {
    pub chip_id: u16,
    pub dev_type: String,
    pub flash_type: String,
    pub flash_pagesize: u32,
    pub sram_size: u32,
    pub dual_bank: bool,
}

impl From<&stlink::ChipDescriptor> for ChipInfo {
    fn from(c: &stlink::ChipDescriptor) -> Self {
        Self {
            chip_id: c.chip_id,
            dev_type: c.dev_type.clone(),
            flash_type: c.flash_type.clone(),
            flash_pagesize: c.flash_pagesize,
            sram_size: c.sram_size,
            dual_bank: c.dual_bank,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashGeometryDto {
    pub base: u32,
    pub size: u32,
    pub bank_size: u32,
    pub sector_size: u32,
    pub write_granularity: u32,
    pub erased_value: u8,
}

impl From<&FlashGeometry> for FlashGeometryDto {
    fn from(g: &FlashGeometry) -> Self {
        Self {
            base: g.base,
            size: g.size,
            bank_size: g.bank_size,
            sector_size: g.sector_size,
            write_granularity: g.write_granularity,
            erased_value: g.erased_value,
        }
    }
}

fn rdp_name(l: RdpLevel) -> &'static str {
    match l {
        RdpLevel::Level0 => "Level0",
        RdpLevel::Level1 => "Level1",
        RdpLevel::Level2 => "Level2",
    }
}

/// UID as 24 hex digits: three 32-bit words, each printed big-endian (as `stlink-cli probe`).
pub fn uid_hex(uid: &[u8; 12]) -> String {
    uid.chunks_exact(4)
        .map(|w| format!("{:08X}", u32::from_le_bytes([w[0], w[1], w[2], w[3]])))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfoDto {
    pub core: CoreInfoDto,
    pub chip: Option<ChipInfo>,
    pub chip_id: u16,
    pub idcode_raw: u32,
    pub rev_id: u16,
    pub flash_size: Option<u32>,
    pub uid: Option<String>,
    pub geometry: Option<FlashGeometryDto>,
    pub rdp: Option<&'static str>,
}

impl TargetInfoDto {
    pub fn new(t: &stlink::TargetInfo, rdp: Option<RdpLevel>) -> Self {
        Self {
            core: (&t.core).into(),
            chip: t.chip.map(ChipInfo::from),
            chip_id: t.chip_id,
            idcode_raw: t.idcode_raw,
            rev_id: t.rev_id(),
            flash_size: t.flash_size,
            uid: t.uid.as_ref().map(uid_hex),
            geometry: stlink::flash::geometry(t)
                .ok()
                .map(|g| FlashGeometryDto::from(&g)),
            rdp: rdp.map(rdp_name),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceSession {
    pub probe: ProbeInfo,
    pub connection: ConnectionDto,
    pub target: TargetInfoDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SectorDto {
    pub bank: u8,
    pub index: u32,
    pub addr: u32,
}

impl From<&Sector> for SectorDto {
    fn from(s: &Sector) -> Self {
        Self {
            bank: s.bank,
            index: s.index,
            addr: s.addr,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashPlan {
    pub addr: u32,
    pub len: u32,
    pub padded_len: u32,
    pub sectors: Vec<SectorDto>,
}

impl FlashPlan {
    pub fn new(plan: &WritePlan, len: u32) -> Self {
        Self {
            addr: plan.addr,
            len,
            padded_len: plan.padded_len,
            sectors: plan.sectors.iter().map(SectorDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProgressDto {
    pub stage: &'static str,
    pub done: u64,
    pub total: u64,
}

impl From<stlink::flash::Progress> for ProgressDto {
    fn from(p: stlink::flash::Progress) -> Self {
        use stlink::flash::Stage;
        Self {
            stage: match p.stage {
                Stage::Erasing => "erasing",
                Stage::Programming => "programming",
                Stage::Verifying => "verifying",
                Stage::Reading => "reading",
            },
            done: p.done,
            total: p.total,
        }
    }
}

/// A firmware file; its id is the absolute path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinFile {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub size: u64,
    pub modified_at: u64,
    pub crc32: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteJob {
    pub file_id: String,
    pub addr: u32,
    pub verify: bool,
    pub reset_after: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StageMs {
    pub erasing: f64,
    pub programming: f64,
    pub verifying: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub plan: FlashPlan,
    pub crc32: u32,
    pub elapsed_ms: f64,
    pub stage_ms: StageMs,
    pub verified: bool,
    pub reset_done: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EraseResult {
    pub banks: u8,
    pub size: u32,
    pub elapsed_ms: f64,
    pub bank_ms: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveTarget {
    pub name: String,
    pub dir: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadJob {
    pub addr: u32,
    pub size: u32,
    pub target: SaveTarget,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    pub addr: u32,
    pub size: u32,
    pub crc32: u32,
    pub elapsed_ms: f64,
    pub saved_to: SaveTarget,
}

/// `DeviceEvent` of `Backend.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DeviceEvent {
    Connected {
        probe: ProbeSummary,
    },
    Disconnected {
        #[serde(rename = "probeId")]
        probe_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_words_are_big_endian_hex() {
        let uid = [
            0x29, 0x00, 0x3A, 0x00, 0x0D, 0x51, 0x33, 0x31, 0x37, 0x38, 0x38, 0x36,
        ];
        assert_eq!(uid_hex(&uid), "003A00293133510D36383837");
    }

    #[test]
    fn device_event_shape() {
        let e = DeviceEvent::Disconnected {
            probe_id: "1-1:7".into(),
        };
        let json = serde_json::to_value(&e).unwrap_or_default();
        assert_eq!(json, serde_json::json!({ "type": "disconnected", "probeId": "1-1:7" }));
    }

    #[test]
    fn plan_keeps_unpadded_len() {
        let plan = WritePlan {
            addr: 0x0800_0000,
            padded_len: 64,
            sectors: vec![Sector {
                bank: 0,
                index: 0,
                addr: 0x0800_0000,
            }],
        };
        let json = serde_json::to_value(FlashPlan::new(&plan, 50)).unwrap_or_default();
        assert_eq!(json["len"], 50);
        assert_eq!(json["paddedLen"], 64);
        assert_eq!(json["sectors"][0]["addr"], 0x0800_0000);
    }
}
