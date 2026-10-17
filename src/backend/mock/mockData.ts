// Bench values from the mockups: ST-Link V2 clone V2J46S7 + STM32H743.

import type { ConnectOptions, Connection, LogLevel, ProbeInfo, ProbeSummary, TargetInfo } from "../types";
import { H743_GEOMETRY } from "../../domain/geometry";

export const MOCK_PROBE: ProbeInfo = {
  id: "1-1:7",
  descriptor: {
    vid: 0x0483,
    pid: 0x3748,
    model: "V2",
    serial: "37FF71064E57343641412543",
    busId: "1-1",
    deviceAddress: 7,
  },
  version: { stlink: 2, jtag: 46, swim: 7, msd: 0, bridge: 0, api: "V2", display: "V2J46S7" },
  targetVoltage: 3.28,
};

export const MOCK_SUMMARY: ProbeSummary = { id: MOCK_PROBE.id, descriptor: MOCK_PROBE.descriptor };

export const MOCK_UID = "003A00293133510D36383837";

export const MOCK_TARGET: TargetInfo = {
  core: { cpuid: 0x411fc271, implementer: 0x41, core: "M7", revision: [1, 1] },
  chip: {
    chipId: 0x450,
    devType: "STM32H74x_H75x",
    flashType: "H7",
    flashPagesize: 0x20000,
    sramSize: 0x100000,
    dualBank: true,
  },
  chipId: 0x450,
  idcodeRaw: 0x1003_6450,
  revId: 0x1003,
  flashSize: H743_GEOMETRY.size,
  uid: MOCK_UID,
  geometry: H743_GEOMETRY,
  rdp: "Level0",
};

export function mockConnection(opts: ConnectOptions): Connection {
  return {
    swdKhz: opts.swdKhz,
    haltedByConnect: opts.underReset,
    lowPowerDebug: opts.keepDebugInLowPower ? true : null,
  };
}

/** The demo firmware already "on the board" and in the quick write card. */
export const DEMO_FILE = { name: "blinky-h743.bin", size: 187_392 } as const;

/** Deterministic pseudo-firmware bytes for a file name. */
export function synthFirmware(name: string, size: number, salt = 0): Uint8Array {
  let x = salt * 0x9e3779b9;
  for (let i = 0; i < name.length; i++) x = (Math.imul(x, 31) + name.charCodeAt(i)) | 0;
  const out = new Uint8Array(size);
  for (let i = 0; i < size; i++) {
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    out[i] = x & 0xff;
  }
  return out;
}

/** Journal lines of the mockup (`LOG` in Main.dc.html), time of day + level + message. */
export const MOCK_LOG: readonly [string, LogLevel, string][] = [
  ["14:31:58.402", "INFO", "probe found: ST-Link V2 0483:3748, serial 37FF71064E57343641412543"],
  ["14:31:58.409", "TRACE", "→ F1 80 00 00 00 00 00 00 00 00 00 00 00 00 00 00 (GET_VERSION)"],
  ["14:31:58.410", "TRACE", "← 2E 87 83 04 48 37 (ST-Link v2, JTAG v46, SWIM v7)"],
  ["14:31:58.415", "INFO", "firmware V2J46S7, api v2, target voltage 3.28 V"],
  ["14:31:58.433", "DEBUG", "swd clock set to 1800 kHz"],
  ["14:31:58.471", "INFO", "target: STM32H74x_H75x (0x450), Cortex-M7 r1p1, flash 2 MiB, rdp 0"],
  ["14:32:03.120", "INFO", "write plan: erase sectors 0-1 of bank 1 (256 KiB)"],
  ["14:32:05.044", "INFO", "erase done in 1.92 s"],
  ["14:32:06.341", "INFO", "programmed 187392 bytes in 1.30 s (141 KiB/s)"],
  ["14:32:06.342", "TRACE", "→ F2 3E 00 00 00 00 00 00 00 00 00 00 00 00 00 00 (GETLASTRWSTATUS2)"],
  ["14:32:06.343", "TRACE", "← 80 00 00 00 00 00 00 00 00 00 00 00 (OK)"],
  ["14:32:07.118", "INFO", "verify ok, crc32 0x9A41C2F7"],
  ["14:32:07.140", "INFO", "reset and run"],
  ["14:35:12.006", "WARN", "usb transfer timed out after 1000 ms, retrying"],
  ["14:35:12.410", "ERROR", "probe disconnected"],
  ["14:35:14.882", "INFO", "probe found: ST-Link V2 0483:3748, serial 37FF71064E57343641412543"],
  ["14:35:14.950", "INFO", "target: STM32H74x_H75x (0x450), Cortex-M7 r1p1, flash 2 MiB, rdp 0"],
];

/** Rust `Display` texts of the errors the dev panel can inject. */
export const MOCK_MESSAGES = {
  Disconnected: "device disconnected",
  CoreNotAccessible:
    "STM32H74x_H75x: core not accessible (firmware in low-power mode?); enable debug in low-power modes or connect under reset",
  FlashProtected: "flash is protected",
  Busy: "USB device is busy (used by another program?)",
  EraseFault: "flash operation failed (status 0x00020000)",
  NotConnected: "device not found",
} as const;
