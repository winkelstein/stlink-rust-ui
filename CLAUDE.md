# stlink-rust-ui

Desktop app for STM32: read/write firmware through an ST-Link (V2 clones, genuine V3) and through USB DFU (DfuSe, system bootloader, no ST-Link).

Full requirements: @docs/PRD-stlink-rust-ui-2026-10-17.md

## Current stage
- Stage 1 (library `stlink/` + test bench `stlink-cli/`): M0–M3 for H7 are done and verified on the bench; full erase is available (`flash::mass_erase`, H7 bank erase ~5.5 s per 1 MB bank).
- Stage 2 (GUI, Tauri 2 + React 19 + TS in `src-tauri/`, `src/`): screens and sheets per `docs/design/`, explicit connect/disconnect, write/read/full erase, quick flash, settings, log, ru/en. See "Stage 2 notes".
- Deferred: M3 for other families (F4/F427, then F1, L4, G4, G0, F0, F7), M4 DFU (no hardware yet).
- Flash write mode is settled (PRD open question 1): direct register writes over SWD in 1 KB chunks, no flash loader. H743: programming ~87 KB/s @1.8 MHz, ~137 KB/s @4 MHz; erase ~0.95 s per 128 KB sector.

## Test bench
- The bench is macOS (Apple Silicon). Windows is supported (platform code behind `cfg(windows)`, verified by CI and the manual checklist in `docs/windows.md`; there is no Windows hardware for development). Linux builds in CI without dedicated support.
- Probe: ST-Link V2 clone `0483:3748`, firmware V2J46S7 (references: `st-info`, `st-flash`, `openocd`).
- Target: STM32H743VIT6 (Cortex-M7, chip id `0x450`, DBGMCU_IDCODE at `0x5C001000` / via AP2 at `0xE00E1000`, 2 MB flash, 2 banks, 128 KB sectors, 32-byte program granularity).
- Bench quirks (important for debugging):
  - The firmware enters low-power with debug clocks off (`DBGMCU_CR = 0`) → AP0 returns stale data **with an OK status**. Use `keep_debug_in_low_power` (once after power-up) or `under_reset`.
  - NRST from the ST-Link does NOT reset the target → connect under reset falls back to AIRCR.SYSRESETREQ (like st-flash).
  - `st-info --connect-under-reset` leaves the core halted at the reset vector.
  - The ST-Link remembers the SWD clock of the previous session → always set it explicitly.
- DFU target later: STM32F427 (`0483:df11`).

## Commands
- Build/check: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p stlink`; frontend: `npm run build`, `npm test`. CI (`.github/workflows/ci.yml`, macOS / Windows / Ubuntu) runs the same and builds installers.
- Windows without Windows: `cargo clippy -p stlink -p stlink-cli --target x86_64-pc-windows-msvc -- -D warnings` (`src-tauri` for Windows is checked in CI only).
- Release: tag `vX.Y.Z` → `.github/workflows/release.yml` (draft Release); the version in `Cargo.toml`, `tauri.conf.json` and `package.json` must match (`node scripts/check-version.mjs vX.Y.Z`). Details: `docs/release.md`.
- Hardware regression: `scripts/hw-test.sh [--erase-all] [firmware.bin]` (no arguments: does not write flash; with a file: writes and verifies; `--erase-all`: full erase and 0xFF check, then the file is written back). Run after changes in `stlink/`.
- GUI backend on hardware: `cargo test -p stlink-rust-ui -- --ignored` (connect → disconnect → `st-info --probe` and openocd work → connect again).
- Bench CLI: `cargo run -p stlink-cli -- list | probe | regs | mem <addr> [count] [--ap N] | read <file> [--addr] [--size] | write <file.bin> [--addr] [--no-verify] [--no-reset] | erase-all`
  - `--connect-under-reset`, `--keep-debug-in-low-power`, `--freq <kHz>` (default 1800), `-v/-vv/-vvv` (`-vvv`: hex trace of every USB packet).
- Commits: English, conventional commits; push only when asked.

## `stlink/` library architecture
| Module | What |
|---|---|
| `usb` | discovery (`list_probes`, serial normalization for clones), `transport` (bulk + timeouts + trace, disconnect detection via the bus) |
| `probe` | `StLink`: ST-Link protocol, version/`ProbeCapabilities` (feature gating), commands, multi-AP |
| `arm` | memory (`read_memory`, `read_u32`/`write_u32` via READMEM/WRITEMEM + status), halt/run, registers; `arm/reset.rs`: connect under reset, `reset_and_run` |
| `chips` | chip table from `chips/stlink/*.chip` (stlink, BSD-3) + `chips/extras.txt`; RDP/DBGMCU tables per `flash_type` |
| `target` | `identify()` → `TargetInfo`; chip lookup via the debug AP (H7: AP2); `enable_debug_in_low_power` |
| `session` | `ConnectOptions` → `StLink::connect` → `Connection`; `release` |
| `flash` | `dump` (flash only, `Error::OutOfFlash`), `read_len`, `plan_write`, `geometry`, `write_flash` (erase → program → verify), `mass_erase` (per bank), `read_protection`; drivers via the `FlashDriver` trait (`flash/h7.rs`), geometry/plan in `flash/geometry.rs` |

## Stage 2 notes (UI)
- **Design:** `docs/design/README.md` (screens, quick-flash rules) and `docs/design/Main.dc.html` (markup, light/dark CSS tokens, copy for every screen). Port to React + TS keeping the structure and tokens.
- **GUI flow** (the CLI does the same, see `stlink-cli/src/commands.rs`):
  `list_probes()` → `StLink::open(&desc)` → `target_voltage()` → `connect(&ConnectOptions)` → `identify()` → `dump(...)` / `plan_write(...)` + confirmation (show `plan.sectors`) → `write_flash(...)` → `reset_and_run()`; for read/probe: `release(&connection)`.
- **Threads:** one worker thread (actor) per device, commands are serialized. `StLink`, `Error`, `ProbeDescriptor` are `Send` (checked at compile time in `lib.rs`), not `Sync`. The whole API is blocking, no async runtime. Dropping `StLink` exits debug mode.
- **Progress and cancellation:** callback `FnMut(Progress) -> ControlFlow<()>`; in Tauri, emit an event and return `Break` when the cancel flag (`AtomicBool`) is set. Cancellation happens between blocks; on `Cancelled`/error during a write the flash is partially written and the core stays halted.
- **Errors:** `stlink::Error` has no `Serialize`; `src-tauri` maps it to `{ kind, message }`. Handle separately: `Disconnected` (reconnect and retry), `CoreNotAccessible` (suggest "keep debug in low-power" or "connect under reset"), `FlashProtected`, `Busy` (probe in use by another program).
- **Confirmations:** before `write_flash`, show which sectors will be erased (`flash::plan_write`), and that `under_reset`/`keep_debug_in_low_power` change the target state (reset; DBGMCU_CR until power-off).
- **Settings** (PRD §6): SWD clock (default `session::DEFAULT_SWD_KHZ`), verify, connect under reset, keep debug in low-power.
- **Logs for bug reports:** tracing-subscriber in Tauri with `trace` level for `stlink` into a file/buffer; silence `nusb` below trace (it duplicates our errors).
- **Build:** `src-tauri` is a workspace member; its `[profile.release]` lives in the root `Cargo.toml` (member profiles are ignored). Run: `npm run tauri dev`; `VITE_BACKEND=mock` runs the frontend on `MockBackend`.
- **GUI backend (`src-tauri/src`):** `device` (actor per probe, `Worker`; the probe is opened only by the `connect` command, `disconnect` drops `StLink`), `hotplug` (`nusb::watch_devices` → rescan → `device-event` with the USB descriptor, without opening), `dto` (mirror of `src/backend/types.ts`), `error` (`{ kind, message }`), `files` (`.bin`, CRC, notify watcher on the directory), `settings` (`quick-write.json` in app data, `settings.json` in app config), `logging` (ring buffer of 5000 lines + `log://entry`, file `~/Library/Logs/<id>/stlink-ui.log`; `stlink` TRACE only with "Verbose USB log", via `reload::Handle`). Progress via `ipc::Channel`, cancellation via `cancel_op(opId)`. Frontend: `src/backend/TauriBackend.ts`, strings in `src/i18n/{ru,en}.ts`.
- **Platforms:** all frontend platform logic is in `src/platform.ts` (⌘/Ctrl, Finder/Explorer); window config in `tauri.macos.conf.json` (overlay titlebar, ad-hoc signing) and `tauri.windows.conf.json` (native frame). On Windows nusb only works with WinUSB: a different driver → `Error::DriverMissing`, "in use" (ACCESS_DENIED/SHARING_VIOLATION) → `Busy`.
- **Not ready in the GUI:** write and erase for families other than H74x/H75x/H72x/H73x (`Error::Unsupported`), DFU.

## Rules
- Language: everything in the repository is in English (docs, comments, commit messages, CLI output, log messages). The only Russian text is the UI translation in `src/i18n/ru.ts` and tests that check Russian formatting.
- USB only through `nusb` (not rusb/libusb). Logging via `tracing`. Errors: `thiserror` in the library, `anyhow` only in the CLI.
- No `panic!`/`unwrap()` in the library on device-dependent paths: release uses `panic = "abort"`. Every USB operation has a timeout; an unplugged cable → `Error::Disconnected`, never a hang.
- Feature gating by ST-Link firmware version: never send commands the firmware does not support (clones often run old versions).
- Read/write single words through `read_u32`/`write_u32` (READMEM/WRITEMEM + GETLASTRWSTATUS2), not `READ_DEBUG_REG`: clones return stale data for it with an OK status.
- While the target is in reset, AP0 on H7 does not respond (garbage data): writes only, no read-modify-write; check DHCSR/DEMCR values for reserved bits.
- Never retry a flash write on WAIT (resending = programming the word again).
- Chip data lives in a table (based on stlink `config/chips/*.chip`, BSD-3), not hardcoded in logic.
- References: stlink (BSD-3, borrowing allowed with credit in `NOTICE`), probe-rs (MIT/Apache). **OpenOCD is GPL-2.0: do not copy code**, only check protocol behavior.
- Firmware formats: `.bin` only (`.hex` later; `.elf` is not supported).
- Never touch Option Bytes/RDP. Mass erase only on explicit request.
- Cover hardware-independent logic (response parsing, version/register decoding, flash geometry, DfuSe descriptors) with unit tests.
