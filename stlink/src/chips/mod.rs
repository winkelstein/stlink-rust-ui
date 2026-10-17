//! STM32 chip table.
//!
//! Data, not code: `chips/stlink/*.chip` are the stlink project's chip files
//! (BSD-3, see `NOTICE`), embedded at build time; `chips/extras.txt` adds the
//! UID address and flash-size register quirks.

use std::sync::OnceLock;

use tracing::warn;

use crate::arm::CoreType;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/chip_files.rs"));
}

const EXTRAS: &str = include_str!("../../chips/extras.txt");

/// Start of main flash on all STM32.
pub const FLASH_BASE: u32 = 0x0800_0000;

/// How to turn the flash size register into bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlashSizeQuirk {
    /// `reg[15:0]` KiB.
    #[default]
    None,
    /// `0` means 128 KiB.
    ZeroIs128K,
    /// `reg[7:0]` KiB.
    LowByte,
    /// L1 Cat.4: `0` → 384 KiB, otherwise 256 KiB.
    L1Cat4,
}

impl FlashSizeQuirk {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "-" => FlashSizeQuirk::None,
            "zero_is_128k" => FlashSizeQuirk::ZeroIs128K,
            "low_byte" => FlashSizeQuirk::LowByte,
            "l1_cat4" => FlashSizeQuirk::L1Cat4,
            _ => return None,
        })
    }

    /// Flash size in bytes from the 16-bit register value.
    pub fn flash_size(self, reg: u16) -> u32 {
        let kib = match self {
            FlashSizeQuirk::None => u32::from(reg),
            FlashSizeQuirk::ZeroIs128K if reg == 0 => 128,
            FlashSizeQuirk::ZeroIs128K => u32::from(reg),
            FlashSizeQuirk::LowByte => u32::from(reg & 0xFF),
            FlashSizeQuirk::L1Cat4 if reg == 0 => 384,
            FlashSizeQuirk::L1Cat4 => 256,
        };
        kib * 1024
    }
}

/// One STM32 device family entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipDescriptor {
    pub chip_id: u16,
    /// Name as printed by `st-info` (`STM32H74x_H75x`).
    pub dev_type: String,
    /// Reference manual number (`0433` → RM0433).
    pub ref_manual: String,
    /// Flash controller family (`H7`, `F2_F4`, ...).
    pub flash_type: String,
    pub flash_size_reg: u32,
    pub flash_pagesize: u32,
    pub sram_size: u32,
    /// System bootloader ROM.
    pub bootrom_base: u32,
    pub bootrom_size: u32,
    pub dual_bank: bool,
    pub swo: bool,
    pub uid_reg: Option<u32>,
    pub flash_size_quirk: FlashSizeQuirk,
}

/// All known chips.
pub fn table() -> &'static [ChipDescriptor] {
    static TABLE: OnceLock<Vec<ChipDescriptor>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let (chips, errors) = build_table();
        for e in errors {
            warn!("chip table: {e}");
        }
        chips
    })
}

/// Look up a chip by DBGMCU_IDCODE[11:0].
pub fn find(chip_id: u16) -> Option<&'static ChipDescriptor> {
    table().iter().find(|c| c.chip_id == chip_id)
}

/// Candidate DBGMCU_IDCODE addresses, in probing order, by core.
pub fn idcode_addresses(core: CoreType) -> &'static [u32] {
    match core {
        // H7 (D3 debug), then F7.
        CoreType::M7 => &[0x5C00_1000, 0xE004_2000],
        // F0/G0/L0/C0.
        CoreType::M0 | CoreType::M0Plus => &[0x4001_5800],
        // L5/U5/H5.
        CoreType::M33 | CoreType::M23 => &[0xE004_4000],
        // F1/F2/F3/F4/L1/L4/G4/WB/WL, and a guess for anything else.
        CoreType::M3 | CoreType::M4 | CoreType::Unknown(_) => &[0xE004_2000],
    }
}

/// Register holding the RDP byte, by `flash_type`: `(address, bit shift)`.
/// Read only — option bytes are never written by this library.
const RDP_REGISTERS: &[(&str, u32, u8)] = &[
    ("H7", 0x5200_201C, 8),    // FLASH_OPTSR_CUR (RM0433)
    ("F2_F4", 0x4002_3C14, 8), // FLASH_OPTCR (RM0090)
    ("F7", 0x4002_3C14, 8),    // FLASH_OPTCR (RM0385)
    ("L4", 0x4002_2020, 0),    // FLASH_OPTR (RM0351)
    ("G4", 0x4002_2020, 0),    // FLASH_OPTR (RM0440)
    ("G0", 0x4002_2020, 0),    // FLASH_OPTR (RM0444)
    ("WB_WL", 0x5800_4020, 0), // FLASH_OPTR (RM0434, RM0461)
];

/// Where DBGMCU_CR is and which bits keep debug alive in Sleep/Stop/Standby.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbgmcuLowPower {
    /// Access port to use (H7: AP2 = APB-D, reachable while the core sleeps).
    pub ap: u8,
    pub addr: u32,
    pub bits: u32,
}

/// DBGMCU low-power debug settings by `flash_type`: `(ap, address, bits)`.
const DBGMCU_LOW_POWER: &[(&str, u8, u32, u32)] = &[
    // DBGMCU_CR via APB-D; DBGSLEEP_D1 | DBGSTOP_D1 | DBGSTBY_D1 | D1DBGCKEN | D3DBGCKEN (RM0433)
    ("H7", 2, 0xE00E_1004, 0x0060_0007),
    // DBG_SLEEP | DBG_STOP | DBG_STANDBY
    ("F2_F4", 0, 0xE004_2004, 0x7),
    ("F7", 0, 0xE004_2004, 0x7),
    ("L4", 0, 0xE004_2004, 0x7),
    ("G4", 0, 0xE004_2004, 0x7),
];

/// DBGMCU_IDCODE reachable through a debug AP without the core's AP0:
/// `(ap, address)`. H7: APB-D on AP2.
const DEBUG_AP_IDCODE: &[(u8, u32)] = &[(2, 0xE00E_1000)];

/// Candidate `(ap, address)` of DBGMCU_IDCODE outside AP0.
pub fn debug_ap_idcode_addresses() -> &'static [(u8, u32)] {
    DEBUG_AP_IDCODE
}

impl ChipDescriptor {
    /// RDP register `(address, shift)`, if known for this family.
    pub fn rdp_register(&self) -> Option<(u32, u8)> {
        RDP_REGISTERS
            .iter()
            .find(|(t, _, _)| *t == self.flash_type)
            .map(|&(_, addr, shift)| (addr, shift))
    }

    /// DBGMCU_CR settings for debugging in low-power modes, if known.
    pub fn dbgmcu_low_power(&self) -> Option<DbgmcuLowPower> {
        DBGMCU_LOW_POWER
            .iter()
            .find(|(t, ..)| *t == self.flash_type)
            .map(|&(_, ap, addr, bits)| DbgmcuLowPower { ap, addr, bits })
    }
}

fn build_table() -> (Vec<ChipDescriptor>, Vec<String>) {
    let mut errors = Vec::new();
    let mut chips: Vec<ChipDescriptor> = embedded::CHIP_FILES
        .iter()
        .filter_map(|(name, text)| {
            parse_chip_file(text)
                .map_err(|e| errors.push(format!("{name}: {e}")))
                .ok()
        })
        .collect();

    for (n, line) in EXTRAS.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        match parse_extras_line(line) {
            Some((id, uid, quirk)) => match chips.iter_mut().find(|c| c.chip_id == id) {
                Some(chip) => {
                    chip.uid_reg = Some(uid);
                    chip.flash_size_quirk = quirk;
                }
                None => errors.push(format!("extras.txt:{}: no chip 0x{id:03x}", n + 1)),
            },
            None => errors.push(format!("extras.txt:{}: cannot parse `{line}`", n + 1)),
        }
    }
    chips.sort_by_key(|c| c.chip_id);
    (chips, errors)
}

fn parse_extras_line(line: &str) -> Option<(u16, u32, FlashSizeQuirk)> {
    let mut it = line.split_whitespace();
    let id = u16::try_from(parse_num(it.next()?)?).ok()?;
    let uid = parse_num(it.next()?)?;
    let quirk = FlashSizeQuirk::parse(it.next()?)?;
    it.next().is_none().then_some((id, uid, quirk))
}

/// Parse one stlink `.chip` file (`key value  // comment`, `#` comments).
fn parse_chip_file(text: &str) -> Result<ChipDescriptor, String> {
    let mut chip = ChipDescriptor {
        chip_id: 0,
        dev_type: String::new(),
        ref_manual: String::new(),
        flash_type: String::new(),
        flash_size_reg: 0,
        flash_pagesize: 0,
        sram_size: 0,
        bootrom_base: 0,
        bootrom_size: 0,
        dual_bank: false,
        swo: false,
        uid_reg: None,
        flash_size_quirk: FlashSizeQuirk::None,
    };
    let mut have_id = false;
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or_default().trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let value = value.trim();
        let num = || parse_num(value).ok_or_else(|| format!("bad number for {key}: `{value}`"));
        match key {
            "dev_type" => chip.dev_type = value.to_owned(),
            "ref_manual_id" => chip.ref_manual = value.to_owned(),
            "chip_id" => {
                chip.chip_id = u16::try_from(num()?).map_err(|_| "chip_id out of range")?;
                have_id = true;
            }
            "flash_type" => chip.flash_type = value.to_owned(),
            "flash_size_reg" => chip.flash_size_reg = num()?,
            "flash_pagesize" => chip.flash_pagesize = num()?,
            "sram_size" => chip.sram_size = num()?,
            "bootrom_base" => chip.bootrom_base = num()?,
            "bootrom_size" => chip.bootrom_size = num()?,
            "flags" => {
                for flag in value.split_whitespace() {
                    match flag {
                        "dualbank" => chip.dual_bank = true,
                        "swo" => chip.swo = true,
                        _ => {}
                    }
                }
            }
            // Option bytes / OTP are deliberately not used.
            _ => {}
        }
    }
    if !have_id || chip.dev_type.is_empty() {
        return Err("missing chip_id or dev_type".into());
    }
    Ok(chip)
}

fn parse_num(s: &str) -> Option<u32> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_table_parses_cleanly() {
        let (chips, errors) = build_table();
        assert!(errors.is_empty(), "{errors:#?}");
        assert!(chips.len() >= 60, "{}", chips.len());
        let mut ids: Vec<_> = chips.iter().map(|c| c.chip_id).collect();
        ids.dedup();
        assert_eq!(ids.len(), chips.len(), "duplicate chip ids");
    }

    #[test]
    fn h743_entry() {
        let c = find(0x450);
        assert_eq!(c.map(|c| c.dev_type.as_str()), Some("STM32H74x_H75x"));
        assert_eq!(c.map(|c| c.flash_size_reg), Some(0x1FF1_E880));
        assert_eq!(c.map(|c| c.flash_pagesize), Some(0x20000));
        assert_eq!(c.map(|c| c.sram_size), Some(0x20000));
        assert_eq!(c.map(|c| c.dual_bank), Some(true));
        assert_eq!(c.and_then(|c| c.uid_reg), Some(0x1FF1_E800));
    }

    #[test]
    fn required_families_present() {
        // F0, F1, F2, F3, F4 (F427/F429), F7, G0, G4, H7, L0, L1, L4, WB
        for id in [
            0x440, 0x410, 0x411, 0x422, 0x419, 0x449, 0x460, 0x468, 0x450, 0x417, 0x416, 0x415,
            0x495,
        ] {
            assert!(find(id).is_some(), "0x{id:03x} missing");
        }
        assert!(
            table().iter().all(|c| c.uid_reg.is_some()),
            "every chip has a UID address"
        );
    }

    #[test]
    fn parse_file_with_comments() {
        let text =
            "# hdr\n#\ndev_type STM32X\nchip_id 0x123   // c\nflash_pagesize 2048\nflags swo\n";
        let c = parse_chip_file(text);
        assert_eq!(c.as_ref().map(|c| c.chip_id).ok(), Some(0x123));
        assert_eq!(c.as_ref().map(|c| c.flash_pagesize).ok(), Some(2048));
        assert_eq!(c.map(|c| (c.swo, c.dual_bank)).ok(), Some((true, false)));
        assert!(parse_chip_file("dev_type X\n").is_err());
        assert!(parse_chip_file("dev_type X\nchip_id zz\n").is_err());
    }

    #[test]
    fn flash_size_quirks() {
        assert_eq!(FlashSizeQuirk::None.flash_size(2048), 2 * 1024 * 1024);
        assert_eq!(FlashSizeQuirk::ZeroIs128K.flash_size(0), 128 * 1024);
        assert_eq!(FlashSizeQuirk::ZeroIs128K.flash_size(64), 64 * 1024);
        assert_eq!(FlashSizeQuirk::LowByte.flash_size(0xFF80), 128 * 1024);
        assert_eq!(FlashSizeQuirk::L1Cat4.flash_size(0), 384 * 1024);
        assert_eq!(FlashSizeQuirk::L1Cat4.flash_size(1), 256 * 1024);
    }

    #[test]
    fn family_registers() {
        let h7 = find(0x450);
        assert_eq!(h7.and_then(|c| c.rdp_register()), Some((0x5200_201C, 8)));
        assert_eq!(
            h7.and_then(|c| c.dbgmcu_low_power()),
            Some(DbgmcuLowPower {
                ap: 2,
                addr: 0xE00E_1004,
                bits: 0x0060_0007
            })
        );
        let f427 = find(0x419);
        assert_eq!(f427.and_then(|c| c.rdp_register()), Some((0x4002_3C14, 8)));
        // Every flash_type in the tables exists in the chip table.
        for t in RDP_REGISTERS
            .iter()
            .map(|r| r.0)
            .chain(DBGMCU_LOW_POWER.iter().map(|r| r.0))
        {
            assert!(table().iter().any(|c| c.flash_type == t), "{t}");
        }
    }

    #[test]
    fn idcode_candidates() {
        assert_eq!(idcode_addresses(CoreType::M7).first(), Some(&0x5C00_1000));
        assert_eq!(idcode_addresses(CoreType::M0Plus), &[0x4001_5800]);
    }
}
