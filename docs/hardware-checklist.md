# Hardware checklist by family

Flash drivers are tested against a register model (`stlink/src/flash/fake.rs`) for every family. This list is what to run on a real board before a family counts as "verified on hardware" in the README.

Status:

| Family | Driver | Verified on hardware |
|---|---|---|
| H74x/H75x, H72x/H73x | `flash/h7.rs` | yes, STM32H743VIT6 |
| F0, F1, F3 (+ F1 XL) | `flash/f0f1.rs` | F1 yes, STM32F103C8T6 (F0, F3, F1 XL not yet) |
| F2, F4 | `flash/f4.rs` | not yet (STM32F427 planned) |
| F7 | `flash/f4.rs` (same controller) | not yet |
| G4 | `flash/g4.rs` | not yet |

## Before you start

- Note the board's firmware so you can restore it: `stlink-cli read backup.bin` (the whole flash).
- Firmware that sleeps with debug clocks off: run once `stlink-cli --keep-debug-in-low-power probe` after power-up, or add `--connect-under-reset`.
- Write at both 1.8 MHz (default) and 4 MHz (`--freq 4000`). Words streamed over SWD faster than the flash programs them can be dropped without any error flag: on the F103 over a third of the halfwords were lost at 1.8 MHz until F0/F1/F3 moved to an SRAM loader. F2/F4/F7 and G4 still stream words over SWD, so a failed verify there points to the same cause.

## Automated

```bash
scripts/hw-test.sh --erase-all backup.bin
```

It reads the chip, flash size and family from `probe` and runs:
- probe vs `st-info`, read at 1.8 and 4 MHz vs `st-flash`, rejected writes (unaligned, too big, not `.bin`);
- erase-all, a check that the whole flash reads `0xFF`, then `backup.bin` written back with verify and reset.

Without `--erase-all` and without a file it does not write flash.

## By hand, per family

For each step note the time (it fills the `TODO(measure)` estimates in `src/domain/estimate.ts`).

1. **probe**: `stlink-cli probe` → `chipid`, `dev-type`, `flash`, `flash-type` and `layout` are right for the part (layout against the reference manual).
   - F42x/F43x 1 MiB, F76x/F77x, G47x/G48x: the layout follows the option bytes (DB1M / nDBANK / DBANK). Check against STM32CubeProgrammer → Option bytes.
   - F0/F1: on a clone (GD32, CKS32, APM32) the log shows `probably a clone`; flashing should still work.
2. **read**: `stlink-cli read all.bin` → same bytes as `st-flash read all-ref.bin 0x08000000 <size>`.
3. **write + verify**: `stlink-cli write fw.bin` → `verified`, the firmware runs after reset.
   - At an address in the middle of a page/sector: the unit is erased whole, the rest of the flash is unchanged (compare a full read before and after).
   - Across a bank boundary (F1 XL at `0x08080000`, F42x 2 MiB at `0x08100000`, G474 dual bank at `0x08040000`): both banks right.
   - F2/F4/F7 at a target voltage below 2.7 V (if the board allows): the log says `x8 parallelism`, the write works.
4. **erase-all**: `stlink-cli erase-all` → every bank erased (`bank n/n`), a full read is all `0xFF`.
5. **write back** `backup.bin` with verify.
6. **reset**: the firmware runs (`write` resets by default; `--no-reset` leaves the core halted).
7. **GUI**: Connect → the flash map shows pages/sectors (grouped for hundreds of pages) → quick flash → Erase all → Disconnect.

## Reporting

Attach the `-vvv` log (`stlink-cli -vvv …`) or the app log (`~/Library/Logs/com.andrewwinkelstein.stlink-rust-ui/stlink-ui.log`), the chip marking and the probe firmware version.
