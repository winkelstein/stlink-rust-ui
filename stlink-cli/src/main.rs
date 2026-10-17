//! Console test bench for the `stlink` library.

use std::io::Write as _;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use stlink::flash::{Progress, RdpLevel, Stage, crc32};
use stlink::{ProbeDescriptor, StLink, TargetInfo};
use tracing_subscriber::EnvFilter;

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

    /// Hold NRST while attaching (resets the target; needs NRST wired).
    /// Use when the firmware disables SWD or sleeps. The core is resumed afterwards.
    #[arg(long, global = true)]
    connect_under_reset: bool,

    /// SWD clock, kHz (rounded down to what the probe supports). Always set
    /// explicitly: the probe otherwise keeps the clock of the previous session.
    /// 4000 is ~1.6x faster if the wiring allows it.
    #[arg(long, global = true, default_value_t = 1800)]
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
    /// Read memory (by default the whole flash) into a .bin file.
    Read {
        /// Output file.
        file: PathBuf,
        /// Start address.
        #[arg(long, value_parser = parse_u32, default_value = "0x08000000")]
        addr: u32,
        /// Bytes to read; default: from `addr` to the end of flash.
        #[arg(long, value_parser = parse_u32)]
        size: Option<u32>,
    },
}

fn parse_u32(s: &str) -> Result<u32, String> {
    let r = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(&hex.replace('_', ""), 16),
        None => s.parse(),
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

/// How to connect to the target.
#[derive(Clone, Copy)]
struct ConnectOpts {
    timeout: Duration,
    under_reset: bool,
    freq_khz: u32,
    keep_debug: bool,
}

impl ConnectOpts {
    /// Set the clock, enter SWD (optionally under reset), apply DBGMCU option.
    fn attach(self, link: &mut StLink) -> anyhow::Result<()> {
        match link.set_swd_frequency(self.freq_khz) {
            Ok(actual) if actual != self.freq_khz => {
                eprintln!("note: SWD clock {actual} kHz (requested {})", self.freq_khz);
            }
            Ok(_) => {}
            // Old firmware: keeps its fixed default clock.
            Err(e @ stlink::Error::Unsupported(_)) => tracing::debug!("{e}"),
            Err(e) => return Err(anyhow::Error::new(e).context("cannot set SWD frequency")),
        }
        // H7: DBGMCU is reachable through AP2 even while the core sleeps,
        // so enable debug clocks before touching the core.
        let mut keep_done = false;
        if self.keep_debug {
            link.enter_swd().context("cannot enter SWD mode")?;
            if let Some(chip) = link.identify_via_debug_ap()?
                && chip.dbgmcu_low_power().is_some_and(|d| d.ap != 0)
            {
                link.enable_debug_in_low_power(chip)?;
                keep_done = true;
            }
        }
        if self.under_reset {
            link.attach_under_reset()
                .context("connect under reset failed")?;
        } else if !link.in_debug_mode() {
            link.enter_swd().context("cannot enter SWD mode")?;
        }
        if self.keep_debug && !keep_done {
            let target = link.identify().context("cannot identify target")?;
            let chip = target
                .chip
                .context("unknown chip: DBGMCU layout not known")?;
            if !link.enable_debug_in_low_power(chip)? {
                eprintln!(
                    "note: DBGMCU low-power bits not known for {}",
                    chip.dev_type
                );
            }
        }
        Ok(())
    }

    /// After connect under reset the core is halted by us; let it run.
    fn finish(self, link: &mut StLink) -> anyhow::Result<()> {
        if self.under_reset {
            link.run().context("cannot resume core")?;
        }
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    let opts = ConnectOpts {
        timeout: Duration::from_millis(cli.timeout_ms),
        under_reset: cli.connect_under_reset,
        freq_khz: cli.freq,
        keep_debug: cli.keep_debug_in_low_power,
    };

    let probes = find_probes(cli.serial.as_deref())?;
    match cli.command {
        Command::List => cmd_list(&probes),
        Command::Probe => cmd_probe(&probes, opts),
        Command::Regs => {
            let desc = probes.first().context("no ST-Link found")?;
            cmd_regs(desc, opts)
        }
        Command::Mem { addr, count, ap } => {
            let desc = probes.first().context("no ST-Link found")?;
            cmd_mem(desc, opts, addr, count, ap)
        }
        Command::Write {
            file,
            addr,
            no_verify,
            no_reset,
        } => {
            let desc = probes.first().context("no ST-Link found")?;
            cmd_write(desc, opts, &file, addr, !no_verify, !no_reset)
        }
        Command::Read { file, addr, size } => {
            let desc = probes.first().context("no ST-Link found")?;
            cmd_read(desc, opts, &file, addr, size)
        }
    }
}

fn find_probes(serial: Option<&str>) -> anyhow::Result<Vec<ProbeDescriptor>> {
    let mut probes = stlink::list_probes().context("USB enumeration failed")?;
    if let Some(serial) = serial {
        probes.retain(|p| p.serial.eq_ignore_ascii_case(serial));
    }
    Ok(probes)
}

fn cmd_list(probes: &[ProbeDescriptor]) -> anyhow::Result<()> {
    println!("Found {} stlink programmers", probes.len());
    for p in probes {
        println!(
            "  {:<14} {:04x}:{:04x}  serial: {}  bus: {} addr: {}",
            p.model.to_string(),
            p.vid,
            p.pid,
            p.serial,
            p.bus_id,
            p.device_address
        );
    }
    Ok(())
}

fn cmd_probe(probes: &[ProbeDescriptor], opts: ConnectOpts) -> anyhow::Result<()> {
    println!("Found {} stlink programmers", probes.len());
    let mut failed = 0;
    for desc in probes {
        if let Err(e) = probe_one(desc, opts) {
            println!("  error:      {e:#}");
            failed += 1;
        }
    }
    if failed > 0 {
        bail!("{failed} of {} probes failed", probes.len());
    }
    Ok(())
}

fn probe_one(desc: &ProbeDescriptor, opts: ConnectOpts) -> anyhow::Result<()> {
    let mut link = StLink::open_with_timeout(desc, opts.timeout).context("cannot open ST-Link")?;
    println!("  version:    {}", link.version());
    println!("  serial:     {}", desc.serial);
    match link.target_voltage() {
        Ok(Some(v)) => println!("  voltage:    {v:.2} V"),
        Ok(None) => println!("  voltage:    n/a"),
        Err(e) => println!("  voltage:    error: {e}"),
    }
    opts.attach(&mut link)?;
    let target = link.identify().context("cannot identify target");
    opts.finish(&mut link)?;
    print_target(&target?);
    Ok(())
}

fn print_target(t: &TargetInfo) {
    let (pagesize, sram, dev_type) = match t.chip {
        Some(c) => (c.flash_pagesize, c.sram_size, c.dev_type.as_str()),
        None => (0, 0, "unknown"),
    };
    println!(
        "  flash:      {} (pagesize: {pagesize})",
        t.flash_size.unwrap_or(0)
    );
    println!("  sram:       {sram}");
    println!("  chipid:     0x{:03x}", t.chip_id);
    println!("  dev-type:   {dev_type}");
    println!("  rev-id:     0x{:04x}", t.rev_id());
    println!("  core:       {}", t.core);
    if let Some(uid) = t.uid {
        let words: Vec<String> = uid
            .chunks_exact(4)
            .map(|w| format!("{:08X}", u32::from_le_bytes([w[0], w[1], w[2], w[3]])))
            .collect();
        println!("  uid:        {}", words.join(" "));
    }
}

fn cmd_regs(desc: &ProbeDescriptor, opts: ConnectOpts) -> anyhow::Result<()> {
    let mut link = StLink::open_with_timeout(desc, opts.timeout).context("cannot open ST-Link")?;
    opts.attach(&mut link)?;
    let core = link.core_info()?;
    if !core.is_arm() {
        // Same diagnosis as `probe` (e.g. H7 core asleep with debug clocks off).
        link.identify().context("cannot access the core")?;
        bail!(stlink::Error::TargetNotConnected);
    }
    let (regs, was_running) = link
        .read_core_registers()
        .context("cannot read core registers")?;
    opts.finish(&mut link)?;

    println!("core: {core}");
    println!(
        "state: {}",
        match (opts.under_reset, was_running) {
            (true, _) => "halted after reset (resumed)",
            (false, true) => "running (halted for reading, resumed)",
            (false, false) => "halted (left halted)",
        }
    );
    for (i, r) in regs.r.iter().enumerate() {
        println!("  r{i:<4} 0x{r:08X}");
    }
    for (name, v) in [
        ("sp", regs.sp),
        ("lr", regs.lr),
        ("pc", regs.pc),
        ("xpsr", regs.xpsr),
        ("msp", regs.msp),
        ("psp", regs.psp),
    ] {
        println!("  {name:<5} 0x{v:08X}");
    }
    Ok(())
}

fn cmd_mem(
    desc: &ProbeDescriptor,
    opts: ConnectOpts,
    addr: u32,
    count: u32,
    ap: u8,
) -> anyhow::Result<()> {
    if addr % 4 != 0 {
        bail!("address must be 4-aligned");
    }
    let mut link = StLink::open_with_timeout(desc, opts.timeout).context("cannot open ST-Link")?;
    opts.attach(&mut link)?;
    if ap != 0 {
        link.init_ap(ap).context("cannot init AP")?;
    }
    for i in 0..count {
        let a = addr.wrapping_add(i * 4);
        let word = link.read_mem32_ap(ap, a, 4).and_then(|b| {
            b.try_into()
                .map(u32::from_le_bytes)
                .map_err(|_| stlink::Error::InvalidResponse("short"))
        });
        match word {
            Ok(v) => println!("0x{a:08X}: 0x{v:08X}"),
            Err(e) => println!("0x{a:08X}: error: {e}"),
        }
    }
    opts.finish(&mut link)
}

fn cmd_read(
    desc: &ProbeDescriptor,
    opts: ConnectOpts,
    file: &PathBuf,
    addr: u32,
    size: Option<u32>,
) -> anyhow::Result<()> {
    let mut link = StLink::open_with_timeout(desc, opts.timeout).context("cannot open ST-Link")?;
    opts.attach(&mut link)?;
    let result = read_to_vec(&mut link, addr, size);
    opts.finish(&mut link)?;
    let (data, elapsed) = result?;

    std::fs::write(file, &data).with_context(|| format!("cannot write {}", file.display()))?;
    let secs = elapsed.as_secs_f64();
    println!(
        "read {} bytes from 0x{addr:08X} in {secs:.2} s ({:.1} KiB/s), CRC32 0x{:08X} → {}",
        data.len(),
        data.len() as f64 / 1024.0 / secs.max(1e-9),
        crc32(&data),
        file.display()
    );
    Ok(())
}

fn read_to_vec(
    link: &mut StLink,
    addr: u32,
    size: Option<u32>,
) -> anyhow::Result<(Vec<u8>, Duration)> {
    let target = link.identify().context("cannot identify target")?;
    let flash_base = stlink::chips::FLASH_BASE;
    let flash_end = target
        .flash_size
        .map(|s| u64::from(flash_base) + u64::from(s));
    let in_flash =
        flash_end.is_some_and(|end| (u64::from(flash_base)..end).contains(&u64::from(addr)));

    if let Some(chip) = target.chip
        && in_flash
    {
        match link.read_protection(chip)? {
            Some(RdpLevel::Level0) | None => {}
            Some(level) => bail!("flash is read-protected ({level:?}): it cannot be read over SWD"),
        }
    }

    let len = match (size, flash_end) {
        (Some(size), _) => u64::from(size),
        (None, Some(end)) if in_flash => end - u64::from(addr),
        _ => bail!("--size is required outside the known flash range"),
    };
    let len = usize::try_from(len).context("size too large")?;

    let start = Instant::now();
    let mut last_pct = u64::MAX;
    let data = link.read_flash(addr, len, |p: Progress| {
        let pct = (p.done * 100).checked_div(p.total).unwrap_or(100);
        if pct != last_pct {
            last_pct = pct;
            eprint!("\rReading {pct:3}% ({}/{} bytes)", p.done, p.total);
            let _ = std::io::stderr().flush();
        }
        ControlFlow::Continue(())
    });
    eprintln!();
    Ok((data?, start.elapsed()))
}

fn cmd_write(
    desc: &ProbeDescriptor,
    opts: ConnectOpts,
    file: &PathBuf,
    addr: u32,
    verify: bool,
    reset: bool,
) -> anyhow::Result<()> {
    if !file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
    {
        bail!("only .bin files are supported");
    }
    let data = std::fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;

    let mut link = StLink::open_with_timeout(desc, opts.timeout).context("cannot open ST-Link")?;
    opts.attach(&mut link)?;
    let target = link.identify().context("cannot identify target")?;
    let chip = target
        .chip
        .context("unknown chip: flash layout not known")?;
    let flash_size = target.flash_size.context("flash size unknown")?;
    // Validate before touching anything (also done again inside write_flash).
    stlink::flash::write_geometry(chip, flash_size)?
        .plan_write(addr, data.len())
        .context("invalid write")?;
    println!(
        "{}: writing {} bytes (CRC32 0x{:08X}) to 0x{addr:08X}",
        chip.dev_type,
        data.len(),
        crc32(&data)
    );

    let start = Instant::now();
    let mut last = (Stage::Reading, u64::MAX);
    // (stage, started, bytes or sectors) for the timing summary.
    let mut stages: Vec<(Stage, Instant, u64)> = Vec::new();
    let result = link.write_flash(chip, flash_size, addr, &data, verify, |p: Progress| {
        if stages.last().is_none_or(|s| s.0 != p.stage) {
            stages.push((p.stage, Instant::now(), p.total));
        }
        let pct = (p.done * 100).checked_div(p.total).unwrap_or(100);
        if (p.stage, pct) != last {
            if p.stage != last.0 && last.1 != u64::MAX {
                eprintln!();
            }
            last = (p.stage, pct);
            eprint!(
                "\r{:<12} {pct:3}% ({}/{})",
                format!("{:?}", p.stage),
                p.done,
                p.total
            );
            let _ = std::io::stderr().flush();
        }
        ControlFlow::Continue(())
    });
    eprintln!();
    match result {
        Ok(()) => {}
        Err(e @ stlink::Error::Disconnected) => {
            return Err(anyhow::Error::new(e).context(
                "flash write interrupted: flash is partially written; reconnect and write again",
            ));
        }
        Err(e) => {
            // Leave the core halted on failure: the flash content is not trusted.
            return Err(anyhow::Error::new(e).context("flash write failed"));
        }
    }
    let end = Instant::now();
    let timing: Vec<String> = stages
        .iter()
        .enumerate()
        .map(|(i, &(stage, t0, total))| {
            let t1 = stages.get(i + 1).map_or(end, |s| s.1);
            let secs = (t1 - t0).as_secs_f64();
            match stage {
                Stage::Erasing => format!("erase {secs:.2} s"),
                _ => format!(
                    "{} {secs:.2} s ({:.1} KiB/s)",
                    format!("{stage:?}").to_lowercase(),
                    total as f64 / 1024.0 / secs.max(1e-9)
                ),
            }
        })
        .collect();
    println!(
        "done in {:.2} s{}: {}",
        start.elapsed().as_secs_f64(),
        if verify { ", verified" } else { "" },
        timing.join(", ")
    );
    if reset {
        link.reset_and_run().context("cannot reset target")?;
        println!("target reset, firmware running");
    }
    Ok(())
}
