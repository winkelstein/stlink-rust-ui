//! Subcommand implementations.

use std::path::Path;

use anyhow::{Context, bail};
use stlink::flash::crc32;
use stlink::{ConnectOptions, Connection, ProbeDescriptor, StLink, TargetInfo};

use crate::progress::{ProgressPrinter, rate};

/// Probe selection and connection settings shared by all commands.
pub struct Settings {
    pub timeout: std::time::Duration,
    pub connect: ConnectOptions,
}

impl Settings {
    fn open(&self, desc: &ProbeDescriptor) -> anyhow::Result<StLink> {
        StLink::open_with_timeout(desc, self.timeout).context("cannot open ST-Link")
    }

    fn connect(&self, link: &mut StLink) -> anyhow::Result<Connection> {
        let c = link
            .connect(&self.connect)
            .context("cannot connect to target")?;
        if let Some(khz) = c.swd_khz
            && khz != self.connect.swd_khz
        {
            eprintln!(
                "note: SWD clock {khz} kHz (requested {})",
                self.connect.swd_khz
            );
        }
        if c.low_power_debug == Some(false) {
            eprintln!("note: DBGMCU low-power bits are not known for this chip");
        }
        Ok(c)
    }
}

pub fn list(probes: &[ProbeDescriptor]) -> anyhow::Result<()> {
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

/// Like `st-info --probe`, plus voltage, revision, core and UID.
pub fn probe(probes: &[ProbeDescriptor], settings: &Settings) -> anyhow::Result<()> {
    println!("Found {} stlink programmers", probes.len());
    let mut failed = 0;
    for desc in probes {
        if let Err(e) = probe_one(desc, settings) {
            println!("  error:      {e:#}");
            failed += 1;
        }
    }
    if failed > 0 {
        bail!("{failed} of {} probes failed", probes.len());
    }
    Ok(())
}

fn probe_one(desc: &ProbeDescriptor, settings: &Settings) -> anyhow::Result<()> {
    let mut link = settings.open(desc)?;
    println!("  version:    {}", link.version());
    println!("  serial:     {}", desc.serial);
    match link.target_voltage() {
        Ok(Some(v)) => println!("  voltage:    {v:.2} V"),
        Ok(None) => println!("  voltage:    n/a"),
        Err(e) => println!("  voltage:    error: {e}"),
    }
    let conn = settings.connect(&mut link)?;
    let target = link.identify().context("cannot identify target");
    link.release(&conn)?;
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

pub fn regs(desc: &ProbeDescriptor, settings: &Settings) -> anyhow::Result<()> {
    let mut link = settings.open(desc)?;
    let conn = settings.connect(&mut link)?;
    let core = link.core_info()?;
    if !core.is_arm() {
        // Same diagnosis as `probe` (e.g. H7 core asleep with debug clocks off).
        link.identify().context("cannot access the core")?;
        bail!(stlink::Error::TargetNotConnected);
    }
    let (regs, was_running) = link
        .read_core_registers()
        .context("cannot read core registers")?;
    link.release(&conn)?;

    println!("core: {core}");
    let state = match (conn.halted_by_connect, was_running) {
        (true, _) => "halted after reset (resumed)",
        (false, true) => "running (halted for reading, resumed)",
        (false, false) => "halted (left halted)",
    };
    println!("state: {state}");
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

pub fn mem(
    desc: &ProbeDescriptor,
    settings: &Settings,
    addr: u32,
    count: u32,
    ap: u8,
) -> anyhow::Result<()> {
    if addr % 4 != 0 {
        bail!("address must be 4-aligned");
    }
    let mut link = settings.open(desc)?;
    let conn = settings.connect(&mut link)?;
    if ap != 0 {
        link.init_ap(ap).context("cannot init AP")?;
    }
    for i in 0..count {
        let a = addr.wrapping_add(i * 4);
        match link.read_u32_ap(ap, a) {
            Ok(v) => println!("0x{a:08X}: 0x{v:08X}"),
            Err(e) => println!("0x{a:08X}: error: {e}"),
        }
    }
    link.release(&conn)?;
    Ok(())
}

pub fn read(
    desc: &ProbeDescriptor,
    settings: &Settings,
    file: &Path,
    addr: u32,
    size: Option<u32>,
) -> anyhow::Result<()> {
    let mut link = settings.open(desc)?;
    let conn = settings.connect(&mut link)?;
    let mut progress = ProgressPrinter::new();
    let data = link
        .identify()
        .context("cannot identify target")
        .and_then(|target| {
            link.dump(&target, addr, size, |p| progress.update(p))
                .context("read failed")
        });
    progress.finish();
    link.release(&conn)?;
    let data = data?;

    std::fs::write(file, &data).with_context(|| format!("cannot write {}", file.display()))?;
    let secs = progress.elapsed_secs();
    println!(
        "read {} bytes from 0x{addr:08X} in {secs:.2} s ({}), CRC32 0x{:08X} → {}",
        data.len(),
        rate(data.len() as u64, secs),
        crc32(&data),
        file.display()
    );
    Ok(())
}

pub fn write(
    desc: &ProbeDescriptor,
    settings: &Settings,
    file: &Path,
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

    let mut link = settings.open(desc)?;
    settings.connect(&mut link)?;
    let target = link.identify().context("cannot identify target")?;
    let plan = stlink::flash::plan_write(&target, addr, data.len()).context("invalid write")?;
    println!(
        "{}: writing {} bytes (CRC32 0x{:08X}) to 0x{addr:08X}, erasing {} sector(s)",
        target.chip.map_or("unknown", |c| c.dev_type.as_str()),
        data.len(),
        crc32(&data),
        plan.sectors.len()
    );

    let mut progress = ProgressPrinter::new();
    let result = link.write_flash(&target, addr, &data, verify, |p| progress.update(p));
    progress.finish();
    // On failure the core stays halted: the flash content is not trusted.
    match result {
        Ok(()) => {}
        Err(e @ stlink::Error::Disconnected) => {
            return Err(anyhow::Error::new(e).context(
                "flash write interrupted: flash is partially written; reconnect and write again",
            ));
        }
        Err(e) => return Err(anyhow::Error::new(e).context("flash write failed")),
    }
    println!(
        "done in {:.2} s{}: {}",
        progress.elapsed_secs(),
        if verify { ", verified" } else { "" },
        progress.summary()
    );
    if reset {
        link.reset_and_run().context("cannot reset target")?;
        println!("target reset, firmware running");
    }
    Ok(())
}

pub fn erase_all(desc: &ProbeDescriptor, settings: &Settings) -> anyhow::Result<()> {
    let mut link = settings.open(desc)?;
    settings.connect(&mut link)?;
    let target = link.identify().context("cannot identify target")?;
    println!(
        "{}: erasing the entire flash ({} bytes)",
        target.chip.map_or("unknown", |c| c.dev_type.as_str()),
        target.flash_size.unwrap_or(0)
    );
    let started = std::time::Instant::now();
    let mut bank_started = started;
    link.mass_erase(&target, |p| {
        if p.done > 0 {
            println!(
                "  bank {}/{} erased in {:.2} s",
                p.done,
                p.total,
                bank_started.elapsed().as_secs_f64()
            );
            bank_started = std::time::Instant::now();
        }
        std::ops::ControlFlow::Continue(())
    })
    .context("erase failed")?;
    println!(
        "flash erased in {:.2} s; core halted",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
