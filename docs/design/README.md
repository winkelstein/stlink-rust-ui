# GUI design (Stage 2)

Source: the "stlink-rust-ui — GUI" canvas in Claude (Design). This folder holds a copy of its sources:

- `Main.dc.html`: **every screen, overlay, the copy in both languages and the navigation logic in one file** (interactive prototype). The main reference for the markup: layout, CSS tokens, the `T` dictionary (ru/en), the `LOG` sample data.
- `canvas.json`: canvas layout and notes (quick-flash rules in `notes.sQuick`, connecting to the probe in `notes.sConn`, settings/log in `notes.sSet`). The other canvas artboards are thin wrappers that show `Main` in a given state (`screen`, `overlay`, `lang`, `dark`), so they are not copied here.
- The `T` dictionary still contains keys of removed features (`autoConnect*`, `onConnect`, `armed*`, `turnOff`): they are not ported to the app.

The `.dc.html` format is not React: `{{hole}}` = values from `renderVals()`, `<sc-if>`/`<sc-for>` = conditionals/loops. Port to React + TS by hand, keeping the structure, tokens and copy.

## General

- Style: native macOS. Font `-apple-system` (SF Pro), monospace `ui-monospace` (SF Mono) for addresses, hex, versions, CRC and the log.
- Theme: System / Light / Dark (setting). Tokens are CSS variables on `.win` / `.win.dark`.
- Language: System / Русский / English (setting). All strings are in the `T` dictionary in `Main.dc.html`; in the app they live in `src/i18n/{ru,en}.ts` with the same keys. Numbers: decimal comma in ru (`3,28 В`), point in en (`3.28 V`).
- Window: 920×620 (minimum size), 52 px titlebar with traffic lights, the stepper and the "Log" and "Settings" buttons; 60 px footer with actions.
- A 3-step wizard: **Device → Options → Run**.

## Screens (`screen`)

| screen | What |
|---|---|
| `empty` | No device, waiting (hot-plug) |
| `released` | A probe is detected but **not opened by the app** (when it appears, after "Disconnect" or a failed connect). "Connect" button, data from the USB descriptor (model, VID:PID, serial). If another program holds the probe: "in use by another program" text and "Retry" |
| `device` | "Quick flash" card; "Programmer" and "Microcontroller" cards; three actions: "Flash firmware", "Read to file", "Erase all" (red). Footer: "connected" status + "Disconnect", on the right "Restart MCU" (hardware reset and run) |
| `eraseConfirm` | Sheet over `device`: erase the entire flash (both banks, addresses), red "Erase all" button |
| `eraseProgress` / `eraseDone` | Stages "Bank 1", "Bank 2" (red progress); result: "Flash erased", MCU halted; "Flash firmware" |
| `write` | .bin file, address, checkboxes "Verify", "Restart MCU", "Re-flash when the file changes"; flash map, time estimate. "Erase and flash" |
| `confirm` | Sheet over `write`: which sectors and addresses will be erased (from `flash::plan_write`) |
| `writeProgress` / `writeDone` | 3 stages: erase, program, verify; result with CRC32, "Flash again ⌘R" |
| `read` / `readProgress` / `readDone` | Range (entire flash / bank 1 / bank 2 / custom), file name, progress, result with CRC32 and "Show in Finder" |

## Overlays (`overlay`, a sheet over any screen)

- **Settings** (titlebar button, ⌘,). Two columns:
  - General: Language, Appearance.
  - Flashing: Verify after writing, Restart MCU after writing, Always confirm before erasing.
  - Programmer: SWD clock (4 MHz / 1.8 MHz / 950 kHz / 480 kHz), Connect under reset, Keep debug in low-power modes. Applied on the next "Connect".
  - Log: Verbose USB log (TRACE).
  Settings are persisted (`settings.json` in the app config directory); "Verify" and "Restart" are the same flags as the checkboxes on the write screen.
- **Log** (titlebar button). Filter All / Errors (WARN+ERROR) / USB (TRACE); "Copy", "Save…", "Clear"; rows: time, level (badge), message in monospace; ERROR rows highlighted. TRACE packets appear only with "Verbose USB log" on. Source: `tracing` from `stlink` (levels as in the library).

## Connecting to the probe

- The app **never connects to an ST-Link on its own**. A detected probe is shown on `released` (USB descriptor data, the device is not opened); the user presses "Connect".
- "Disconnect" in the `device` footer closes the device (drop `StLink` → exit debug, USB interface released) so VS Code, OpenOCD or st-flash can take the ST-Link. Goes to `released`.
- Cable unplugged → `empty`; plugged back → `released`.
- If another program holds the device (`Error::Busy`): `released` with "in use by another program" and a "Retry" button (`TODO(design)`: this variant is not drawn, same layout).

## Quick flash rules

- The last file and address are remembered per chip (by UID).
- "Flash ⌘R" starts flashing right away, without the wizard. The confirmation is skipped only when the same sectors are erased as in the last confirmed write and "Always confirm before erasing" is off; otherwise the regular `confirm` sheet is shown.
- "When the file changes": the .bin is watched; flashing starts ~1 s after the last change (debounce) and does not start if the CRC is unchanged, another operation is running or the probe is disconnected.
- There is no flashing on board connect: the app never opens the probe on its own.

## App icon

Source `src-tauri/icons/app-icon.svg` (+ `app-icon.png` at 1024). The full Tauri set (`icon.icns`, `icon.ico`, PNGs) is generated by `tauri icon` into `src-tauri/icons/`. After a change: `npm run tauri icon src-tauri/icons/app-icon.png`.

## Values in the mockups

Serial number, UID, CRC, file names, timings and log lines are examples. Real values come from `stlink` (`ProbeInfo`, `TargetInfo`, `Progress`, tracing).

## Implementation

The frontend (`src/`) follows the prototype in React 19 + TypeScript without UI kits: tokens and shared classes from `Main.dc.html` live in `src/styles/tokens.css`, screen markup is ported with the same sizes, copy is in the `src/i18n/` dictionaries. The backend is `src-tauri` on top of the `stlink` library; in the browser (and with `VITE_BACKEND=mock`) `MockBackend` is used.

| Where | What |
|---|---|
| `src/backend/types.ts` | Tauri command DTOs mirroring `stlink` types (`ProbeSummary`/`ProbeInfo`, `TargetInfo`, `FlashPlan`, `Progress`, `EraseResult`, `BackendError { kind, message }`), `AppSettings`, `LogEntry` |
| `src/backend/Backend.ts` | Backend interface: hot-plug, `connect`/`disconnect`, `planWrite`, `write`/`read`/`eraseAll` with progress and `AbortSignal`, `resetAndRun`, files (picker, drag-and-drop, watching), quick flash per UID, settings, log |
| `src/backend/TauriBackend.ts` | `invoke` + events (`device-event`, `log://entry`), progress via `Channel`, cancellation via `cancel_op` |
| `src/backend/mock/` | `MockBackend`: the bench (ST-Link V2J46S7 + STM32H743, 3.28 V) on timers, speeds from CLAUDE.md (bank erase 5.5 s/MB), log from `LOG`, settings in `localStorage` |
| `src/i18n/` | `ru.ts`/`en.ts` (keys as in `T`; strings with example values are functions), `core.ts` (language: "System" → `navigator.languages`, `ru*` → ru, otherwise en; numbers and units per language), `index.tsx` (`I18nProvider`, `useT()` → `{ t, f, lang }`) |
| `src/domain/` | Pure logic with tests: port of `plan_write`, address validation, time estimates, per-language formatting, quick-flash rules |
| `src/state/machine.ts` | State machine: screens `empty`, `released`, `device`, `erase*`, `write*`, `read*`; overlays `settings`/`log`; error banners without copy (`kind` + context + `message`) |
| `src/state/controller.ts` | Side effects: backend calls, cancellation, file-watch debounce, persisting settings |
| `src/components/`, `src/screens/` | Shared components (`WindowChrome`, `Sheet`, `ConfirmSheet`, `FlashMap`, `ErrorBanner`, …), screens and the `SettingsSheet`, `LogSheet` sheets |
| `src/dev/DevPanel.tsx` | Mock-mode dev panel |

**Connecting.** Hot-plug comes from `nusb::watch_devices`; a detected probe is not opened. "Connect" → `connect` (open → voltage → connect → identify → RDP, geometry) with the probe options from Settings; "Disconnect" → `disconnect` (drop `StLink`). The CoreNotAccessible banner offers to connect again under reset or with debug kept in low-power (one-off, settings are not changed).

**Flash families.** The geometry is banks of page or sector runs of any size (`FlashGeometry.banks`, `unit: "page" | "sector"`), the same as in the library. The flash map draws blocks proportional to their bytes and groups hundreds of pages into at most 32 blocks per bank. Every text that names the erase unit (address hint, map legend, confirmation, progress, done screen, device card) says pages or sectors, in both languages. Time estimates are per flash family (`speedFor` in `src/domain/estimate.ts`; H7 measured, the others `TODO(measure)`). The mock has benches for H743, F103C8, F411CE, F427, G474 and F746 (dev panel → Board).

**Erase all.** `flash::mass_erase`: bank erase per bank (H7: BER + START; F2/F4/F7: MER/MER1; G4: MER1/MER2; F0/F1: MER per register set), progress per bank, cancellation between banks. Within a bank the bar fills by the time estimate (or by the previous bank's time). After erasing, the core is halted.

**Log.** `src-tauri` has its own `tracing` layer: a ring buffer of 5000 lines, new lines batched into the `log://entry` event at most every 100 ms; the same filter writes the file `~/Library/Logs/com.andrewwinkelstein.stlink-rust-ui/stlink-ui.log`. `stlink` TRACE (USB packets) is toggled on the fly by "Verbose USB log" (`reload::Handle`), `nusb` is silenced. Command errors go to the log with their `message`; the UI shows text by `kind` from the dictionary, with `message` under "Details".

**Read.** The range is checked against the connected chip's flash (`readableFlash`: geometry, otherwise flash size): start inside flash, end not past the end of flash; an empty size means "to the end of flash"; the field shows the maximum from the current address; "Read" is disabled on an error. The Tauri command and the library check the same (`Error::OutOfFlash`).

**The window does not scroll.** `html, body, #root`: `overflow: hidden`, `overscroll-behavior: none`; only screen content and the log list scroll (`overscroll-behavior: contain`). Zoom is disabled: `zoomHotkeysEnabled: false`, Ctrl/⌘+wheel, pinch and Ctrl/⌘ +/−/0 are intercepted (`src/noZoom.ts`).

**Platforms.** `src/platform.ts` is the only place that detects the OS (`navigator.userAgentData`, otherwise `navigator.platform`): shortcut modifier and labels (⌘ / Ctrl), "Show in Finder / in Explorer / in folder". Download paths come from the Tauri path API, native on Windows (`C:\Users\…`), joined with the native separator. Fonts: SF / Segoe UI Variable / Segoe UI, monospace SF Mono / Cascadia Mono / Consolas. Windows drivers: `docs/windows.md`.

**Run:** `npm run dev` (browser, http://localhost:1420, mock) or `npm run tauri dev`. Checks: `npm run build` (tsc + vite), `npm test` (vitest: `domain/`, i18n, reducer, platform). CI and releases: `docs/release.md`.

**Dev panel** exists only in mock mode: **⌥⌘D** (Alt+Ctrl+D outside macOS). From it you can plug and unplug the probe, simulate "in use by another program", a rebuild of the watched file (new or same CRC), an error for the next operation (`Disconnected` in the middle of a write, read or erase, `CoreNotAccessible`, `FlashProtected`, `Busy`, an error in the middle of a full erase), a speed multiplier ×1/×5/×20 and the language. To reset stored data, delete the `stlink-ui.*` keys in `localStorage`.

**Keyboard:** ⌘R (Ctrl+R on Windows and Linux): quick flash on the "Device" and "Done" screens; ⌘, (Ctrl+,): Settings; Esc closes a sheet; in confirmation sheets Enter confirms and Tab stays inside the sheet.

**Tauri window:** 920×620 (also the minimum size). macOS (`tauri.macos.conf.json`): `titleBarStyle: Overlay` + `hiddenTitle`, system traffic lights over our titlebar, which has `data-tauri-drag-region`; the drawn dots appear only in the browser on macOS. Windows (`tauri.windows.conf.json`): native frame with system buttons, our strip below it, not draggable. Native controls (checkboxes, select) follow the app theme (`color-scheme` on `.win`).

**Mock-mode limitations:** the browser has no save dialog: "Change…" asks for a file name, read results and the log are downloaded.

**Not drawn in the mockups** (marked `TODO(design)` in the code): error banners, the "busy" and "WinUSB driver needed" variants of `released`, the MCU card while connecting, the quick-flash card without a file, an MCU without a flash driver (flash and erase tiles disabled with a hint).
