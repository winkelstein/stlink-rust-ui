//! Flash layout templates and bank-mode registers (`chips/layouts.txt`).

use std::sync::OnceLock;

use tracing::warn;

use crate::flash::SectorRun;

const LAYOUTS: &str = include_str!("../../chips/layouts.txt");

/// How a layout template maps onto banks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankMode {
    /// One bank.
    Single,
    /// Two equal banks.
    Dual,
    /// Banks filled in order up to the flash size.
    Fill,
}

/// A layout row: per-bank runs for the largest part of a chip line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutTemplate {
    pub chip_id: u16,
    pub mode: BankMode,
    pub banks: Vec<Vec<SectorRun>>,
}

/// Option register bit that selects single or dual bank (read only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankModeRegister {
    pub addr: u32,
    pub bit: u8,
    /// Dual bank when the bit is set (else when it is clear).
    pub dual_when_set: bool,
}

impl BankModeRegister {
    pub fn is_dual(&self, reg: u32) -> bool {
        ((reg >> self.bit) & 1 == 1) == self.dual_when_set
    }
}

struct Tables {
    layouts: Vec<LayoutTemplate>,
    bank_modes: Vec<(u16, BankModeRegister)>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let (tables, errors) = parse(LAYOUTS);
        for e in errors {
            warn!("layouts.txt: {e}");
        }
        tables
    })
}

/// Layout templates of a chip (one per bank mode).
pub fn layouts(chip_id: u16) -> Vec<&'static LayoutTemplate> {
    tables()
        .layouts
        .iter()
        .filter(|l| l.chip_id == chip_id)
        .collect()
}

/// Where the chip's bank mode is configured, if it has one.
pub fn bank_mode_register(chip_id: u16) -> Option<BankModeRegister> {
    tables()
        .bank_modes
        .iter()
        .find(|(id, _)| *id == chip_id)
        .map(|&(_, r)| r)
}

fn parse(text: &str) -> (Tables, Vec<String>) {
    let mut tables = Tables {
        layouts: Vec::new(),
        bank_modes: Vec::new(),
    };
    let mut errors = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let parsed = if let Some(rest) = line.strip_prefix("bankmode") {
            parse_bank_mode(rest).map(|r| tables.bank_modes.push(r))
        } else {
            parse_layout(line).map(|l| tables.layouts.push(l))
        };
        if parsed.is_none() {
            errors.push(format!("line {}: cannot parse `{line}`", n + 1));
        }
    }
    (tables, errors)
}

fn parse_num(s: &str) -> Option<u32> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

fn parse_layout(line: &str) -> Option<LayoutTemplate> {
    let mut parts = line.split_whitespace();
    let chip_id = u16::try_from(parse_num(parts.next()?)?).ok()?;
    let mode = match parts.next()? {
        "single" => BankMode::Single,
        "dual" => BankMode::Dual,
        "fill" => BankMode::Fill,
        _ => return None,
    };
    let runs = parts.collect::<Vec<_>>().join(" ");
    let banks = runs
        .split('|')
        .map(|bank| {
            bank.split_whitespace()
                .map(|run| {
                    let (count, kib) = run.split_once('x')?;
                    Some(SectorRun {
                        count: count.parse().ok()?,
                        size: kib.parse::<u32>().ok()?.checked_mul(1024)?,
                    })
                })
                .collect::<Option<Vec<_>>>()
                .filter(|runs| !runs.is_empty())
        })
        .collect::<Option<Vec<_>>>()?;
    let expected_banks = if mode == BankMode::Single { 1 } else { 2 };
    (banks.len() == expected_banks).then_some(LayoutTemplate {
        chip_id,
        mode,
        banks,
    })
}

fn parse_bank_mode(rest: &str) -> Option<(u16, BankModeRegister)> {
    let mut it = rest.split_whitespace();
    let chip_id = u16::try_from(parse_num(it.next()?)?).ok()?;
    let addr = parse_num(it.next()?)?;
    let bit = u8::try_from(parse_num(it.next()?)?)
        .ok()
        .filter(|b| *b < 32)?;
    let dual_when_set = match it.next()? {
        "set" => true,
        "clear" => false,
        _ => return None,
    };
    it.next().is_none().then_some((
        chip_id,
        BankModeRegister {
            addr,
            bit,
            dual_when_set,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_layouts_parse_cleanly() {
        let (t, errors) = parse(LAYOUTS);
        assert!(errors.is_empty(), "{errors:#?}");
        assert!(t.layouts.len() >= 15);
        // Every chip id exists in the chip table.
        for id in t
            .layouts
            .iter()
            .map(|l| l.chip_id)
            .chain(t.bank_modes.iter().map(|b| b.0))
        {
            assert!(crate::chips::find(id).is_some(), "0x{id:03x}");
        }
    }

    #[test]
    fn f427_templates() {
        let l = layouts(0x419);
        assert_eq!(l.len(), 2);
        let dual = l.iter().find(|l| l.mode == BankMode::Dual);
        assert_eq!(dual.map(|d| d.banks.len()), Some(2));
        assert_eq!(
            dual.map(|d| d.banks[1].clone()),
            Some(vec![
                SectorRun {
                    count: 4,
                    size: 16 * 1024
                },
                SectorRun {
                    count: 1,
                    size: 64 * 1024
                },
                SectorRun {
                    count: 7,
                    size: 128 * 1024
                },
            ])
        );
        let reg = bank_mode_register(0x419);
        assert_eq!(reg.map(|r| (r.addr, r.bit)), Some((0x4002_3C14, 30)));
        assert_eq!(reg.map(|r| r.is_dual(1 << 30)), Some(true));
        assert_eq!(reg.map(|r| r.is_dual(0)), Some(false));
        // F76x: nDBANK clear means dual bank.
        assert_eq!(bank_mode_register(0x451).map(|r| r.is_dual(0)), Some(true));
    }

    #[test]
    fn rejects_bad_rows() {
        assert!(
            parse_layout("0x419 single 4x16 | 4x16").is_none(),
            "single with two banks"
        );
        assert!(
            parse_layout("0x419 dual 4x16").is_none(),
            "dual with one bank"
        );
        assert!(parse_layout("0x419 triple 4x16").is_none());
        assert!(parse_layout("0x419 single 4y16").is_none());
        assert!(parse_bank_mode(" 0x419 0x40023C14 40 set").is_none());
        assert!(parse_bank_mode(" 0x419 0x40023C14 30 maybe").is_none());
    }
}
