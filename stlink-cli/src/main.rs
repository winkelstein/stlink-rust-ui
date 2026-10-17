//! Console test bench for the `stlink` library.

mod commands;
mod progress;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use stlink::{ConnectOptions, ProbeDescriptor};
use tracing_subscriber::EnvFilter;

use commands::Settings;

#[derive(Parser)]
#[command(version, about = "ST-Link / STM32 DFU test bench")]
struct Cli {
    /// Log level: -v info, -vv debug, -vvv trace (hex dump of every USB packet).
    /// RUST_LOG overrides this.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Use only the probe with this serial number.
    #[arg(long, global = true)]
    serial: Option<String>,

    /// Reset the target while attaching (NRST, or AIRCR.SYSRESETREQ if NRST is
    /// not wired). Use when the firmware disables SWD or sleeps. The core is
    /// resumed afterwards.
    #[arg(long, global = true)]
    connect_under_reset: bool,

    /// SWD clock, kHz (rounded down to what the probe supports). Always set
    /// explicitly: the probe otherwise keeps the clock of the previous session.
    /// 4000 is ~1.6x faster if the wiring allows it.
    #[arg(long, global = true, default_value_t = stlink::session::DEFAULT_SWD_KHZ)]
    freq: u32,

    /// Set DBGMCU_CR so debug keeps working in Sleep/Stop/Standby (until power-off).
    /// Lets later runs work without --connect-under-reset on low-power firmware.
    #[arg(long, global = true)]
    keep_debug_in_low_power: bool,

    /// Timeout of one USB transfer, ms.
    #[arg(long, global = true, default_value_t = 1000)]
    timeout_ms: u64,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected ST-Link probes.
    List,
    /// Identify probe and target (like `st-info --probe`).
    Probe,
    /// Halt the core, print core registers, restore its run state.
    Regs,
    /// Read 32-bit words from target memory (does not halt the core).
    Mem {
        /// Start address (hex with 0x, or decimal); must be 4-aligned.
        #[arg(value_parser = parse_u32)]
        addr: u32,
        /// Number of words.
        #[arg(default_value_t = 1)]
        count: u32,
        /// Access port (0 = AHB-AP of the core; H7: 2 = APB-D debug bus).
        #[arg(long, default_value_t = 0)]
        ap: u8,
    },
    /// Read flash (by default all of it) into a .bin file. The range must lie
    /// in flash; use `mem` for other memory.
    Read {
        /// Output file.
        file: PathBuf,
        /// Start address.
        #[arg(long, value_parser = parse_u32, default_value = "0x08000000")]
        addr: u32,
        /// Bytes to read; default: from `addr` to the end of flash. Must not
        /// go past the end of flash.
        #[arg(long, value_parser = parse_u32)]
        size: Option<u32>,
    },
    /// Write a .bin file to flash: erase, program, verify, reset.
    Write {
        /// Input file (.bin).
        file: PathBuf,
        /// Flash address to write to.
        #[arg(long, value_parser = parse_u32, default_value = "0x08000000")]
        addr: u32,
        /// Skip read-back verification.
        #[arg(long)]
        no_verify: bool,
        /// Leave the core halted instead of resetting it.
        #[arg(long)]
        no_reset: bool,
    },
    /// Erase the entire flash (every bank). Destructive; the core stays halted.
    EraseAll,
}

/// `0x0800_0000`, `0X100` or decimal.
fn parse_u32(s: &str) -> Result<u32, String> {
    let r = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(&hex.replace('_', ""), 16),
        None => s.replace('_', "").parse(),
    };
    r.map_err(|e| e.to_string())
}

fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // nusb's own error logs duplicate our errors; show them from -vv.
        let nusb = if verbose >= 2 { level } else { "off" };
        EnvFilter::new(format!(
            "warn,nusb={nusb},stlink={level},stlink_cli={level}"
        ))
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_target(verbose >= 2)
        .with_writer(std::io::stderr)
        .init();
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    let settings = Settings {
        timeout: Duration::from_millis(cli.timeout_ms),
        connect: ConnectOptions {
            swd_khz: cli.freq,
            under_reset: cli.connect_under_reset,
            keep_debug_in_low_power: cli.keep_debug_in_low_power,
        },
    };

    let probes = find_probes(cli.serial.as_deref())?;
    let first = || probes.first().context("no ST-Link found");
    match cli.command {
        Command::List => commands::list(&probes),
        Command::Probe => commands::probe(&probes, &settings),
        Command::Regs => commands::regs(first()?, &settings),
        Command::Mem { addr, count, ap } => commands::mem(first()?, &settings, addr, count, ap),
        Command::Read { file, addr, size } => {
            commands::read(first()?, &settings, &file, addr, size)
        }
        Command::Write {
            file,
            addr,
            no_verify,
            no_reset,
        } => commands::write(first()?, &settings, &file, addr, !no_verify, !no_reset),
        Command::EraseAll => commands::erase_all(first()?, &settings),
    }
}

fn find_probes(serial: Option<&str>) -> anyhow::Result<Vec<ProbeDescriptor>> {
    let mut probes = stlink::list_probes().context("USB enumeration failed")?;
    if let Some(serial) = serial {
        probes.retain(|p| p.serial.eq_ignore_ascii_case(serial));
    }
    Ok(probes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numbers() {
        assert_eq!(parse_u32("0x08000000"), Ok(0x0800_0000));
        assert_eq!(parse_u32("0X0800_0000"), Ok(0x0800_0000));
        assert_eq!(parse_u32("4096"), Ok(4096));
        assert_eq!(parse_u32("1_024"), Ok(1024));
        assert!(parse_u32("0xZZ").is_err());
        assert!(parse_u32("").is_err());
        assert!(parse_u32("0x1_0000_0000").is_err());
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
