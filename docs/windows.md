# stlink-rust-ui on Windows

The app talks to USB through [`nusb`](https://docs.rs/nusb), and on Windows nusb works **only with devices bound to the WinUSB driver**. libusb/libusbK, the old ST `STLinkUSB` driver and others do not work.

## Drivers

| Device | What to install | How to check |
|---|---|---|
| ST-Link V2 / V2-1 / V3 (`0483:3748`, `374B`, `3752`, `374E`, `374F`, `3753`, `3754`) | ST driver [STSW-LINK009](https://www.st.com/en/development-tools/stsw-link009.html). STM32CubeProgrammer installs it too. It is WinUSB with an INF from ST | Device Manager → "Universal Serial Bus devices" → "STM32 STLink". Properties → Driver → "Driver Details": `winusb.sys` |
| STM32 in DFU mode (`0483:df11`, BOOT0 = 1) | WinUSB via [Zadig](https://zadig.akeo.ie/): Options → List All Devices → "STM32 BOOTLOADER" → WinUSB → Replace Driver | Device Manager → "Universal Serial Bus devices" → "STM32 BOOTLOADER", driver `winusb.sys`. Note: STM32CubeProgrammer does not see DFU devices on this driver, it needs ST's "STM32 DFU" driver; switch back with Zadig or "Roll Back Driver" |

V2-1 and V3 are composite devices (`usbccgp`): WinUSB must be bound to the debug interface (MI_00). Mass storage and the COM port keep their standard drivers.

If the device is visible but has the wrong driver, the app shows the "WinUSB driver needed" screen with the driver name and links to the ST driver and Zadig. The log then contains a `device driver is not WinUSB` line with the driver name.

## Programmer is busy

WinUSB opens a device exclusively. While another program holds the ST-Link (OpenOCD, STM32CubeProgrammer, st-util, the VS Code debugger), the app shows "Programmer is busy" with a Retry button. "Disconnect" in the app closes the device so other programs can take it.

## SmartScreen

Builds are not signed yet. On the first launch of the installer Windows shows "Windows protected your PC": "More info" → "Run anyway". Code signing is described in [release.md](release.md).

## Manual test checklist

Setup: ST-Link (ST driver), STM32H743 board, installer from the CI artifacts (`.msi` or `-setup.exe`).

1. **Install and launch.** Get past SmartScreen with "More info". A 920×620 window with the system frame and Windows caption buttons. Below the frame, our strip with the stepper, "Log" and "Settings", without the colored dots on the left. Nothing is cut off vertically.
2. **Hot-plug.** Without a probe: "Connect a programmer". Plug in the ST-Link → "Programmer disconnected" with the serial number (the device is not opened). Unplug → back to "Connect a programmer".
3. **Connect.** "Connect" → programmer card (version, voltage) and MCU card (STM32H743, UID, RDP 0).
4. **Flash.** "Flash firmware" → pick a `.bin` (Windows dialog) or drop a file from Explorer onto the window → "Erase and flash" → confirmation → progress → "Firmware flashed and verified", CRC32.
5. **Quick flash.** Ctrl+R. Buttons and tooltips say Ctrl+R, not ⌘R.
6. **File watching.** Turn on "When the file changes", rebuild the firmware (or overwrite the file via a temp file + rename). Flashing starts once after ~1 s; it does not start if the CRC is unchanged.
7. **Read.** "Read to file" → "Entire flash" → path like `C:\Users\…\Downloads\…`, with `\` → "Done" → "Show in Explorer" opens Explorer with the file selected. "Range" with a size larger than flash keeps "Read" disabled.
8. **Erase all.** "Erase all" → red confirmation → "Bank 1", "Bank 2" → "Flash erased". Then flash the firmware back.
9. **Disconnect → OpenOCD.** "Disconnect" → `openocd -f interface/stlink.cfg -f target/stm32h7x.cfg -c "init; shutdown"` finds the Cortex-M7 while the app is open → close OpenOCD → "Connect" in the app works.
10. **Busy.** Start OpenOCD (without `shutdown`) → "Connect" → "Programmer is busy" → close OpenOCD → "Retry" works.
11. **Cable pulled.** Unplug the cable while flashing → banner "The connection was lost while flashing", the app does not hang. Plug it back → "Connect" → flashing works.
12. **Driver.** (If you have a second ST-Link or can temporarily install libusbK via Zadig.) "WinUSB driver needed" screen, the links open in the browser.
13. **Appearance and language.** Settings (Ctrl+,): "Dark" → all screens dark, checkboxes and the clock list dark too. Русский → all copy in Russian, numbers with a comma (`3,28 В`). Esc closes Settings.
14. **Scrolling and zoom.** Mouse wheel and touchpad: the window does not scroll or bounce, the log scrolls. Ctrl+wheel, Ctrl+"+"/"−" and pinch do not zoom.
15. **Log.** Filters "All / Errors / USB", "Verbose USB log" → TRACE lines, "Copy", "Save…" (Windows dialog), "Clear".
16. **Log file.** `%LOCALAPPDATA%\com.andrewwinkelstein.stlink-rust-ui\logs\stlink-ui.log` is created. Attach it to bug reports.
