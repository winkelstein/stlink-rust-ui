# PRD: stlink-rust-ui

> Version: 0.2 · Created: 2026-10-17 · Authors: Andrew + Claude
> Status: Stage 1 (library) and Stage 2 (GUI) are implemented for STM32H7; next up is flash support for more families (F0, F1, F4, G4, F7) and DFU.
>
> Changes in 0.2: translated to English; Windows moved from "future" to supported; open question 1 resolved (direct register writes); the GUI section points to `docs/design/README.md`; milestone M3b added for more families.

---

## 1. Overview and goals

**stlink-rust-ui** is a convenient, minimal and stable desktop app for STM32 microcontrollers:

1. through an **ST-Link** probe (V2 clones and genuine V3): chip identification, firmware read/write;
2. through **USB DFU** of the STM32 built-in system bootloader (no ST-Link): firmware write/read.

The project has three parts:

| Part | What | Stage |
|---|---|---|
| `stlink` | Rust library: USB transport, ST-Link protocol, SWD/ARM debug, flash, DfuSe | 1 |
| `stlink-cli` | Command-line test bench for checking the library on hardware | 1 |
| `src-tauri` + `src/` | GUI on Tauri 2 + React + TypeScript | 2 |

### Goals
- **G1.** The baseline: show at least what `st-info --probe` shows, for most STM32 chips.
- **G2.** Reliably read and write flash through an ST-Link (`.bin` format).
- **G3.** Write flash through USB DFU (DfuSe); first target chip: STM32F427.
- **G4.** A modern, minimal GUI on top of the library.
- **G5.** Stability: no hangs when the cable is unplugged, clear errors, clean recovery.

### Non-goals (v1)
- Linux as a first-class platform (it builds in CI, but is not tested; see §11).
- `.hex` (later) and `.elf` (never) formats.
- GDB server, breakpoints, stepping, SWO/tracing.
- JTAG (SWD only), SWIM/STM8.
- Updating the ST-Link's own firmware.
- Changing Option Bytes / RDP (read and warn only).

---

## 2. Target audience

- Embedded developers and hobbyists working with STM32 on macOS and Windows who find STM32CubeProgrammer (heavy, Java) and the command-line `st-flash`/`openocd` inconvenient.
- Owners of cheap ST-Link V2 clones who want predictable behavior without workarounds.
- Users flashing boards over USB DFU (BOOT0=1) without a probe.

---

## 3. Test bench

| Component | Value | Comment |
|---|---|---|
| Host | macOS (Apple Silicon) | The user runs and verifies on hardware |
| Probe | ST-Link "V2" clone (`0483:3748`) | Works correctly with `st-info` and `openocd` |
| Target #1 | STM32H743VIT6 | Cortex-M7, chip id `0x450`, 2 MB flash in 2 banks |
| Target #2 (DFU) | STM32F427 | Arrives later; DFU via USB OTG FS (PA11/PA12) |
| Reference | `st-info --probe`, `openocd` | Our tool's output is compared against them |

**Development process:** code is written directly in the project folder; the build is checked with `cargo check` / CI; hardware runs are done by the user, and the `-vvv` log is sent back.

---

## 4. Features

Features are grouped by library module; the `F*` numbers are used in the milestones.

### F1. ST-Link discovery (USB)

- Enumerate USB devices through `nusb`, filter by VID `0x0483` and known PIDs:

| PID | Model | EP OUT / IN |
|---|---|---|
| `0x3748` | ST-Link V2 (and most clones) | `0x02` / `0x81` |
| `0x374B`, `0x3752` | ST-Link V2-1 | `0x01` / `0x81` |
| `0x374E`, `0x374F`, `0x3753`, `0x3754` | ST-Link V3 (variants) | `0x01` / `0x81` |

- Return a `ProbeDescriptor` (see §6) for each device: PID, model, serial number, bus/address.
- **Clone serial numbers:** many clones report raw bytes instead of ASCII. Normalize like `st-info`: if the string is not printable, print the hex representation of the bytes.
- Hot-plug support via `nusb::watch_devices` (`Connected` / `Disconnected` events).
- **Acceptance criteria:**
  - `stlink-cli list` shows the connected clone with PID `0x3748` and a serial number matching `st-info --probe`.
  - With no devices: an empty list and exit code 0, no panic.
  - Unplugging during work results in `Error::Disconnected`, not a hang.

### F2. ST-Link protocol (probe layer)

- Commands are a 16-byte packet on bulk OUT, the response comes from bulk IN; every operation has a timeout (1000 ms by default, configurable).
- Key commands (check values against `stlink` and OpenOCD):

| Command | Code | Purpose |
|---|---|---|
| `GET_VERSION` | `0xF1` | Version: ST-Link / JTAG / SWIM + VID/PID |
| `GET_VERSION_APIV3` | `0xFB` | Extended version for V3 |
| `GET_CURRENT_MODE` | `0xF5` | Mode: DFU / Mass / Debug / SWIM / Bootloader |
| `DFU` / `DFU_EXIT` | `0xF3` / `0x07` | Leave the ST-Link's own DFU mode |
| `GET_TARGET_VOLTAGE` | `0xF7` | Target voltage: `2 × adc1 × 1.2 / adc0` |
| `DEBUG` | `0xF2` | Prefix for debug subcommands (below) |

- Debug subcommands needed in Stage 1: `ENTER(SWD)`, `EXIT`, `SWD_SET_FREQ` (APIv2) / `SET_COM_FREQ` (APIv3), `READ_IDCODES`, `READMEM_32BIT`, `WRITEMEM_32BIT`, `READMEM_8BIT`, `READ_DEBUG_REG`/`WRITE_DEBUG_REG`, `FORCEDEBUG`, `RUNCORE`, `RESETSYS`, `READREG`, `READALLREGS`, `GETLASTRWSTATUS(2)`, `DRIVE_NRST`.
- **Feature gating by firmware version.** After `GET_VERSION` the library builds a `ProbeCapabilities` set (APIv1/v2/v3, 16-bit memory access, multi-AP, `GETLASTRWSTATUS2`, max read packet size, etc.). Commands the firmware does not support are never sent; a workaround or `Error::Unsupported` is used instead. Version thresholds come from `stlink`/OpenOCD.
- **Initialization:** `GET_CURRENT_MODE` → if the ST-Link is in DFU mode, send `DFU_EXIT`; if in Debug, `EXIT` → then `ENTER(SWD)`.
- **Status handling:** every response with a status ≠ `0x80` (OK) maps to a typed error (`SwdFault`, `ApWait`, `DpWait`, ...). `WAIT` gets a bounded number of retries.
- **Logging/tracing (mandatory from day one):** via `tracing`. At `trace` level every packet is logged as `→ <hex> (<command name>)` / `← <hex> (<decoded>)` with its duration. In the CLI the level is set with `-v`, `-vv`, `-vvv`.
- **Acceptance criteria:**
  - The version is shown in `st-info` format: `V2J37S7` (V2) or `V3J7M3...` (V3) and matches `st-info --probe`.
  - Target voltage is shown with 0.01 V precision (≈3.3 V on the bench).
  - Running the CLI again right after a previous run (including one interrupted with Ctrl+C) works without replugging USB.

### F3. ARM debug layer (SWD / ADIv5 / Cortex-M)

- 8/32-bit memory read/write (16-bit if the firmware supports it) with automatic packet splitting and alignment handling.
- Core control: halt, run, reset (system reset and hardware reset via NRST), reading core registers R0–R12, SP, LR, PC, xPSR, MSP, PSP (via `READREG` or DCRSR/DCRDR).
- Core identification by `CPUID` (`0xE000ED00`): Cortex-M0/M0+/M3/M4/M7/M33.
- **Connect under reset** (optional): hold NRST during `ENTER(SWD)`, for firmware that disables SWD or puts the chip into low-power.
- **Acceptance criteria:**
  - For H743: `CPUID` = `0x411FC271` (Cortex-M7 r1p1) or another M7 revision.
  - Core registers are read after halt; PC/SP values are sane (SP ∈ RAM, PC ∈ flash/RAM).
  - After `stlink-cli regs` the core is back in its original state (if it was running, it keeps running).

### F4. Chip identification (`st-info --probe` equivalent)

- Determine `chip_id` by reading DBGMCU_IDCODE at a family-dependent address. Algorithm: try addresses from a table:

```
determine the core from CPUID
for each candidate address for this core:
    read u32; dev_id = value & 0xFFF
    if dev_id is in the chip table → found
otherwise → Error::UnknownChip(raw)
```

  Typical addresses: `0xE0042000` (F1/F2/F3/F4/F7/L1/L4/G4/WB…), `0x40015800` (F0/G0/L0/C0), `0x5C001000` (H7), `0xE0044000` (U5/H5 and newer, check the table).
- **Chip table** is data, not code: `chip_id`, `dev_type` (string as in `st-info`), family, flash size register address, UID address, SRAM size, flash layout (banks/sectors/pages), system bootloader address. Source: `config/chips/*.chip` from the `stlink` project (BSD-3, may be borrowed with credit).
- `stlink-cli probe` output (`st-info --probe` format plus extra fields):

```
Found 1 stlink programmers
  version:    V2J37S7
  serial:     ...
  voltage:    3.28 V
  flash:      2097152 (pagesize: 131072)
  sram:       ...
  chipid:     0x450
  dev-type:   STM32H74x_H75x
  core:       Cortex-M7 r1p1 (CPUID 0x411FC271)
  uid:        ...
```

- **Acceptance criteria:**
  - On H743 the `version`, `serial`, `flash`, `chipid`, `dev-type` fields match `st-info --probe`.
  - An unknown chip does not break the output: `chipid` is shown with `dev-type: unknown`.
  - The table covers at least: F0, F1, F2, F3, F4 (incl. F427/F429), F7, G0, G4, H7, L0, L1, L4, WB.

### F5. Reading flash to `.bin` (dump)

- Read a range of flash into a file; the whole flash by default. The range must stay inside flash (`Error::OutOfFlash`); arbitrary memory is read with `stlink-cli mem`.
- Progress via callback (`bytes_done`, `bytes_total`).
- Cortex-M7 cache (H7/F7): reads go over the AHB-AP bus, not through the core, so there is no issue; when verifying after a write, invalidate the D-cache if the core is running.
- Warn when RDP is enabled (reads return an error/zeros).
- **Acceptance criteria:** an H743 dump matches `st-flash read` of the same range byte for byte.

### F6. Writing flash from `.bin` through ST-Link

- Input: a `.bin` file + base address (default: start of flash, `0x08000000`).
- Steps: validation (the file fits in flash, the address is aligned) → halt → unlock → erase the affected sectors/pages → program → lock → verify (read back + compare or CRC32) → reset/run (optional).
- Flash drivers per family through a shared `FlashDriver` trait. Order: **H7** (bench, done), then F4 (F427), F1, G4, F7, F0, then L4, G0.
- H7 specifics: 2 banks, 128 KB sectors, 256-bit (32-byte) program words; the buffer is padded with `0xFF` to a multiple of 32.
- Write mode (decided): direct flash register writes over SWD in 1 KB chunks (H7, F2/F4/F7, G4). F0/F1/F3 program through a small loader in SRAM: halfwords streamed over SWD faster than the flash programs them are dropped without an error (seen on an F103 at 1.8 MHz).
- Progress by stage: `Erasing`, `Programming`, `Verifying`; cancellation between blocks.
- Full erase (mass erase) per bank, only on explicit request with a confirmation.
- **Acceptance criteria:**
  - Writing 64 KB to H743 + verification passes; the program runs after reset.
  - A USB drop during a write → a clear error; writing again after reconnecting succeeds.
  - A file larger than flash or a misaligned address → an error before erasing starts.

### F7. USB DFU (DfuSe), no ST-Link

- Detect STM32 system bootloader devices: `0483:df11`.
- Parse interface/alt-setting strings like `@Internal Flash  /0x08000000/04*016Kg,01*064Kg,07*128Kg` → memory layout (address, sector count and size, r/e/w attributes).
- DfuSe operations: `SET_ADDRESS_POINTER`, `ERASE` (per sector), `DNLOAD` in blocks (`wTransferSize` from the functional descriptor), `UPLOAD` for verification, `GETSTATUS`/`CLRSTATUS`/`ABORT`, leaving DFU ("leave", run the firmware).
- Hot-plug: the device appears after a reset with BOOT0=1.
- First target chip: **STM32F427** (sectors 4×16 KB, 1×64 KB, 7×128 KB per bank).
- On Windows the DFU device must use WinUSB (Zadig), see `docs/windows.md`.
- **Acceptance criteria:**
  - `stlink-cli dfu list` shows the F427 in DFU mode with its parsed memory layout.
  - Writing a `.bin` + verification via UPLOAD matches; after "leave" the firmware runs.
  - DfuSe logic (descriptor parsing, states) is covered by unit tests without hardware.

### F8. GUI (Stage 2)

Implemented; the design and behavior are described in `docs/design/README.md` (screens, overlays, quick-flash rules, connect/disconnect, implementation notes). Key points:
- Hot-plug without a "Refresh" button; the app never connects to a probe on its own: "Connect" / "Disconnect" are explicit, so other tools (VS Code, OpenOCD) can use the probe.
- Flash, read, erase all, restart MCU; quick flash (⌘R / Ctrl+R) and re-flash on file change.
- Settings, log (with a verbose USB trace for bug reports), ru/en, light/dark theme.
- Confirmation before destructive operations.

---

## 5. Tech stack

| Layer | Technology | Why |
|---|---|---|
| USB | [`nusb`](https://docs.rs/nusb) 0.2.x | Pure Rust, no libusb; async; hot-plug; used by probe-rs |
| Logging | [`tracing`](https://docs.rs/tracing) + `tracing-subscriber` | Structured logs, levels, hex packet trace |
| Errors | [`thiserror`](https://docs.rs/thiserror) (library), `anyhow` (CLI) | Typed errors for the GUI |
| CLI | [`clap`](https://docs.rs/clap) | Subcommands `list`, `probe`, `regs`, `mem`, `read`, `write`, `erase-all`, `dfu ...` |
| Async runtime | A dedicated worker thread per device + `futures-lite::block_on` for nusb; `tauri::async_runtime` in Tauri | Keep tokio out of the library |
| GUI | Tauri 2 + React 19 + TypeScript + Vite | Already set up in the repository |
| CI/CD | GitHub Actions: macOS / Windows / Ubuntu; tagged releases publish installers, CLI archives, checksums and a GHCR OCI artifact | See `docs/release.md` |
| References | [stlink](https://github.com/stlink-org/stlink) (BSD-3), [OpenOCD](https://openocd.org) (GPL-2.0, protocol knowledge only), [probe-rs](https://github.com/probe-rs/probe-rs) (MIT/Apache) | Protocol, clone workarounds, chip tables |
| ST documentation | RM0433 (H7), RM0090 (F42x/F43x), RM0368 (F401/F411), RM0008 (F1), RM0091 (F0), RM0440 (G4), RM0385/RM0410 (F7), AN3156 (USB DFU), AN2606 (system bootloader), UM1075 (DfuSe) | Flash registers, DfuSe, bootloader |

**Repository layout (Cargo workspace):**

```
stlink-rust-ui/
├── Cargo.toml          # [workspace]: stlink, stlink-cli, src-tauri
├── stlink/             # library
│   └── src/ usb/ probe/ arm/ chips/ flash/ target.rs session.rs error.rs
├── stlink-cli/         # command-line bench
├── src-tauri/          # Tauri backend
├── src/                # React frontend
└── docs/               # PRD, design, Windows and release notes
```

**Licensing:** no OpenOCD code is copied (GPL). Data/code from `stlink` and `probe-rs` is allowed with credit in `NOTICE`.

---

## 6. Conceptual data model

| Entity | Fields (type) | Note |
|---|---|---|
| `ProbeDescriptor` | `vid: u16`, `pid: u16`, `model: ProbeModel {V2, V2_1, V3}`, `serial: String`, `bus_id: String`, `device_address: u8` | Discovery result, the device is not opened |
| `ProbeVersion` | `stlink: u8`, `jtag: u8`, `swim: Option<u8>`, `msd: Option<u8>`, `bridge: Option<u8>`, `api: ApiVersion {V1, V2, V3}` | `Display` → `V2J37S7` |
| `ProbeCapabilities` | flag set: `mem_16bit`, `multi_ap`, `get_last_rw_status2`, `set_freq`, `max_read_size: usize`, … | Derived from `ProbeVersion` |
| `ProbeInfo` | `descriptor: ProbeDescriptor`, `version: ProbeVersion`, `target_voltage: Option<f32>` | |
| `ChipDescriptor` | `chip_id: u16`, `dev_type: &str`, `family: Family`, `idcode_addr: u32`, `flash_size_addr: u32`, `uid_addr: Option<u32>`, `sram_size: u32`, `flash_base: u32`, `flash_layout: FlashLayout`, `bootloader_addr: Option<u32>` | Static table |
| `FlashLayout` | `banks: Vec<FlashBank { base: u32, sectors: Vec<SectorRun { count: u32, size: u32 }> }>`, `write_granularity: u32`, `erased_value: u8` | Shared by ST-Link and DFU |
| `CoreInfo` | `cpuid: u32`, `core: CoreType {M0, M0Plus, M3, M4, M7, M33}`, `revision: (u8, u8)` | |
| `TargetInfo` | `chip: Option<&ChipDescriptor>`, `chip_id_raw: u32`, `core: CoreInfo`, `flash_size: u32`, `uid: Option<[u8; 12]>` | `probe` result |
| `CoreRegisters` | `r: [u32; 13]`, `sp`, `lr`, `pc`, `xpsr`, `msp`, `psp` (`u32`) | |
| `DfuDevice` | `descriptor` (as for Probe), `alt_settings: Vec<DfuAlt { alt: u8, name: String, layout: FlashLayout, attrs }>`, `transfer_size: u16`, `dfu_version: u16` | |
| `FlashJob` | `source: PathBuf`, `base_address: u32`, `size: u32`, `crc32: u32`, `verify: bool`, `reset_after: bool` | |
| `Progress` | `stage: Stage {Erasing, Programming, Verifying, Reading}`, `done: u64`, `total: u64` | Callback/event to the GUI |
| `Error` | `NotFound`, `AccessDenied`, `Disconnected`, `Timeout`, `Usb(..)`, `ProbeStatus(u8)`, `SwdFault`, `TargetNotConnected`, `UnknownChip(u32)`, `Unsupported(&str)`, `FlashProtected`, `OutOfFlash`, `Verify { addr: u32 }`, `Busy`, `DriverMissing`, `Dfu(..)` | Typed, no `panic!` |

**GUI persisted data:** settings (SWD clock, verify/restart defaults, connect under reset, keep debug in low-power, language, theme, verbose log) and quick-flash state per chip UID (path, last address, last confirmed sectors), as JSON in the app config/data directories.

---

## 7. UI principles

- Minimalism: one main window, a 3-step wizard, no nested menus.
- Device state is always visible; hot-plug without a "Refresh" button.
- Destructive actions require a confirmation that states exactly what will be erased.
- Light and dark theme (follows the system by default).
- Errors in plain language + "Details" (technical details from the log).

---

## 8. Security and reliability

- No network, no accounts: minimal attack surface. The app sends no data anywhere.
- **Never** change RDP/Option Bytes in v1. RDP Level 2 is irreversible and is forbidden at the library level.
- Mass erase only on explicit request.
- Validate the file and address before any destructive action.
- Every USB operation has a timeout; on disconnect, resources are released, no panics (`panic = "abort"` in release makes panics fatal).
- Single-threaded access to a device: commands are serialized through the device worker/actor.

---

## 9. Milestones

| # | Name | Content | Done when | Status |
|---|---|---|---|---|
| **M0** | Skeleton | Cargo workspace, `stlink` + `stlink-cli` crates, `tracing`, errors | `stlink-cli --help` works | Done |
| **M1** | Discovery + probe + registers | F1, F2 (basic commands), F3 (memory, halt, registers), F4 (chip table) | `stlink-cli probe` on H743 matches `st-info --probe`; `stlink-cli regs` prints core registers | Done |
| M2 | Flash read | F5, progress, SWD clock | The dump matches `st-flash read` | Done |
| M3 | Flash write (ST-Link), H7 | F6 for H7, full erase | Write + verify on H743 | Done |
| **M3b** | More families | F6 for F0, F1, F4, G4, F7 (plus F2, F3 on the same controllers) | Write + verify + erase all on each family (`docs/hardware-checklist.md`) | Drivers done and tested against a register model; F1 verified on an STM32F103C8T6; the others pending (F427 next) |
| M4 | DFU | F7 on F427 | Write + verify via DfuSe | Waiting for hardware |
| M5 | GUI | F8: Tauri commands and events, React screens | The full read/write flow from the GUI | Done |
| M6 | Platforms and formats | Windows (done), CI/CD (done), `.hex`, Linux polish | See §11 | In progress |

---

## 10. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Clones respond non-standardly / old firmware | Feature gating by version; trace log; compare with `st-info`/OpenOCD |
| H7: several APs, old V2 firmware supports AP0 only | AP0 (AHB-AP D1) is enough for identification and flash; multi-AP only when the firmware supports it |
| H7: DBGMCU at a non-standard address | Table-driven IDCODE address lookup |
| A target whose firmware sleeps or disables SWD | Connect under reset (requires a wired NRST) |
| Cortex-M7 cache during verification | Read via AHB-AP; halt and invalidate if needed |
| No hardware in the development environment | `cargo check` in a VM; unit tests without hardware; trace logs from the user; CI on three OSes |
| DfuSe: F4 bootloader quirks (statuses, erase delays) | Poll `GETSTATUS` honoring `bwPollTimeout`; tests on emulated responses |
| Windows: wrong USB driver, exclusive access | `Error::DriverMissing` with instructions; explicit "Disconnect"; `docs/windows.md` |
| OpenOCD's GPL license | Protocol knowledge only, no code copied |

---

## 11. Future extensions

- **Linux:** udev rules; AppImage/deb polish (CI already builds them).
- **`.hex`** (Intel HEX) with multiple segments.
- Option Bytes (read → edit with safeguards).
- Multi-AP / access to the H7 D3 domain; SWO output; RTT console.
- Batch flashing of several boards, project profiles.
- ST-Link V3 bridge functions (UART/I2C/SPI).
- Code signing and notarization for releases.

---

## 12. Open questions

1. ~~Flash write mode through ST-Link: direct (registers) or a RAM flash loader.~~ Decided: direct register writes in 1 KB chunks (H743: ~87 KB/s at 1.8 MHz, ~137 KB/s at 4 MHz); F0/F1/F3 use an SRAM loader (F103: ~21.5 KB/s).
2. Should `stlink-cli` have JSON output (for tests and integrations)?
3. Minimum macOS version.
4. Should the GUI support several ST-Links connected at once.
