import { describe, expect, it } from "vitest";
import { initialState, reducer, stepOf, type Action, type AppState, type PendingWrite } from "./machine";
import { MOCK_PROBE, MOCK_SUMMARY, MOCK_TARGET, mockConnection } from "../backend/mock/mockData";
import { DEFAULT_QUICK } from "../domain/quickWrite";
import { H743_GEOMETRY, planWrite } from "../domain/geometry";
import type { BinFile, DeviceSession, FlashPlan, WriteResult } from "../backend/types";

const session: DeviceSession = {
  probe: MOCK_PROBE,
  connection: mockConnection({ swdKhz: 1800, underReset: false, keepDebugInLowPower: false }),
  target: MOCK_TARGET,
};
const file: BinFile = { id: "f1", name: "blinky.bin", path: null, size: 187_392, modifiedAt: 0, crc32: 0x1234 };
const r = planWrite(H743_GEOMETRY, 0x0800_0000, file.size);
const plan = (r.ok ? r.plan : null) as FlashPlan;
const pending: PendingWrite = {
  file,
  plan,
  origin: "wizard",
  job: { fileId: file.id, addr: plan.addr, verify: true, resetAfter: true },
};
const result: WriteResult = {
  plan,
  crc32: 0x1234,
  elapsedMs: 4100,
  stageMs: { erasing: 1900, programming: 1300, verifying: 900 },
  verified: true,
  resetDone: true,
};

function run(...actions: Action[]): AppState {
  return actions.reduce(reducer, initialState);
}

const connected: Action[] = [
  { type: "probeConnected", probe: MOCK_SUMMARY },
  { type: "connectStarted" },
  { type: "connected", session, quick: { ...DEFAULT_QUICK, file } },
];

describe("hot-plug and connection", () => {
  it("a found probe is only shown: released until «Подключиться»", () => {
    const s1 = run(connected[0]);
    expect(s1.screen).toBe("released");
    expect(s1.session).toBeNull();
    expect(stepOf(s1.screen)).toBe(1);
    const s = run(...connected);
    expect(s.screen).toBe("device");
    expect(s.connecting).toBe(false);
    expect(s.session).toEqual(session);
    expect(s.writeForm.file).toEqual(file);
  });

  it("unplugging returns to empty, plugging in again to released", () => {
    const s = run(...connected, { type: "probeDisconnected", probeId: MOCK_PROBE.id });
    expect(s.screen).toBe("empty");
    expect(s.session).toBeNull();
    expect(s.quick).toBeNull();
    expect(reducer(s, connected[0]).screen).toBe("released");
  });

  it("«Отключить» closes the session and shows released", () => {
    const s = run(...connected, { type: "released" });
    expect(s.screen).toBe("released");
    expect(s.session).toBeNull();
    expect(s.probe?.id).toBe(MOCK_PROBE.id);
  });

  it("Busy on connect: released with the busy text, no banner", () => {
    const s = run(connected[0], connected[1], { type: "connectFailed", error: { kind: "Busy", message: "busy" } });
    expect(s.screen).toBe("released");
    expect(s.blocked?.reason).toBe("busy");
    expect(s.banner).toBeNull();
    expect(reducer(s, { type: "connectStarted" }).blocked).toBeNull();
  });

  it("Windows driver missing: released with the driver variant", () => {
    const s = run(connected[0], connected[1], {
      type: "connectFailed",
      error: { kind: "DriverMissing", message: 'USB driver "STLinkUSB" cannot be used' },
    });
    expect(s.screen).toBe("released");
    expect(s.blocked).toEqual({ reason: "driver", message: 'USB driver "STLinkUSB" cannot be used' });
  });

  it("disconnect during write: banner with retry, late results ignored", () => {
    const s = run(
      ...connected,
      { type: "writeStarted", id: 1, pending, at: 0 },
      { type: "probeDisconnected", probeId: MOCK_PROBE.id },
      { type: "opFailed", id: 1, error: { kind: "Disconnected", message: "device disconnected" } },
    );
    expect(s.screen).toBe("empty");
    expect(s.op).toBeNull();
    expect(s.banner?.kind).toBe("Disconnected");
    expect(s.banner?.actions).toEqual(["retryWrite"]);
    // The banner survives reconnecting so the write can be retried.
    const back = reducer(reducer(s, connected[0]), connected[1]);
    expect(back.banner?.kind).toBe("Disconnected");
  });

  it("connect failure shows CoreNotAccessible hints on released", () => {
    const s = run(connected[0], connected[1], {
      type: "connectFailed",
      error: { kind: "CoreNotAccessible", message: "x" },
    });
    expect(s.screen).toBe("released");
    expect(s.banner?.context).toBe("connect");
    expect(s.banner?.actions).toEqual(["connectUnderReset", "keepDebugLowPower"]);
  });
});

describe("write flow", () => {
  it("wizard: write → confirm → progress → done", () => {
    const s = run(
      ...connected,
      { type: "navigate", to: "write" },
      { type: "confirmRequested", pending, returnTo: "write" },
    );
    expect(s.screen).toBe("confirm");
    expect(stepOf(s.screen)).toBe(2);
    const s2 = [
      { type: "confirmAccepted" } as Action,
      { type: "writeStarted", id: 1, pending, at: 0 } as Action,
      { type: "opProgress", id: 1, progress: { stage: "programming", done: 10, total: 100 }, at: 5 } as Action,
    ].reduce(reducer, s);
    expect(s2.screen).toBe("writeProgress");
    expect(s2.quick?.lastConfirmedSectors).toEqual(plan.sectors);
    expect(s2.op?.kind === "write" && s2.op.stageStart.programming).toBe(5);
    const s3 = reducer(s2, { type: "writeFinished", id: 1, result });
    expect(s3.screen).toBe("writeDone");
    expect(s3.quick?.lastWrittenCrc).toBe(0x1234);
    expect(s3.quick?.addr).toBe(0x0800_0000);
  });

  it("confirm cancel returns where it came from", () => {
    const base = run(...connected);
    expect(
      [{ type: "confirmRequested", pending, returnTo: "device" } as Action, { type: "confirmCancelled" } as Action].reduce(
        reducer,
        base,
      ).screen,
    ).toBe("device");
  });

  it("cancel during write → write form with the partial-write notice", () => {
    const s = run(
      ...connected,
      { type: "writeStarted", id: 7, pending, at: 0 },
      { type: "opFailed", id: 7, error: { kind: "Cancelled", message: "cancelled" } },
    );
    expect(s.screen).toBe("write");
    expect(s.banner?.kind).toBe("notice");
    expect(s.banner?.context).toBe("write");
  });

  it("stale progress of another op is ignored", () => {
    const s = run(...connected, { type: "writeStarted", id: 2, pending, at: 0 });
    expect(reducer(s, { type: "opProgress", id: 1, progress: { stage: "verifying", done: 1, total: 1 }, at: 1 })).toBe(s);
  });

  it("navigation is blocked while an operation runs", () => {
    const s = run(...connected, { type: "writeStarted", id: 1, pending, at: 0 });
    expect(reducer(s, { type: "navigate", to: "device" })).toBe(s);
    expect(reducer(s, { type: "writeStarted", id: 2, pending, at: 0 })).toBe(s);
  });

  it("errors land on the form with retry", () => {
    const s = run(
      ...connected,
      { type: "writeStarted", id: 1, pending, at: 0 },
      { type: "opFailed", id: 1, error: { kind: "Busy", message: "busy" } },
    );
    expect(s.screen).toBe("write");
    expect(s.banner?.actions).toEqual(["retryWrite"]);
    const p = run(
      ...connected,
      { type: "writeStarted", id: 1, pending, at: 0 },
      { type: "opFailed", id: 1, error: { kind: "FlashProtected", message: "flash is protected" } },
    );
    expect(p.banner?.actions).toEqual([]);
  });

  it("choosing a file opens the write form", () => {
    const s = run(...connected, { type: "fileChosen", file: { ...file, id: "f2", name: "other.bin" } });
    expect(s.screen).toBe("write");
    expect(s.writeForm.file?.name).toBe("other.bin");
  });

  it("verify/resetAfter are settings shared with the form", () => {
    const s = run(...connected, { type: "settingsChanged", patch: { verify: false } });
    expect(s.settings.verify).toBe(false);
    expect(s.settings.resetAfter).toBe(true);
  });
});

describe("read flow", () => {
  it("custom fields editable only in custom mode", () => {
    const s = run(...connected, { type: "navigate", to: "read" });
    expect(reducer(s, { type: "readFieldChanged", field: "startText", text: "0x1" })).toBe(s);
    const c = reducer(s, { type: "readRangeChanged", range: "custom", startText: "0x08000000", sizeText: "0x40000" });
    expect(reducer(c, { type: "readFieldChanged", field: "sizeText", text: "1M" }).readForm.sizeText).toBe("1M");
  });

  it("read → progress → done", () => {
    const job = { addr: 0x0800_0000, size: 0x1000, target: { name: "a.bin", dir: "~/Downloads", path: null } };
    const s = run(
      ...connected,
      { type: "navigate", to: "read" },
      { type: "readStarted", id: 3, job, at: 0 },
      { type: "readFinished", id: 3, result: { addr: job.addr, size: job.size, crc32: 1, elapsedMs: 10, savedTo: job.target } },
    );
    expect(s.screen).toBe("readDone");
    expect(stepOf(s.screen)).toBe(3);
  });
});

describe("erase all", () => {
  const result = { banks: 2, size: 2 << 20, elapsedMs: 11_000, bankMs: [5500, 5500] };

  it("device → confirm → progress by bank → done", () => {
    const s = run(...connected, { type: "navigate", to: "eraseConfirm" });
    expect(s.screen).toBe("eraseConfirm");
    expect(stepOf(s.screen)).toBe(1);
    const p = run(...connected, { type: "navigate", to: "eraseConfirm" }, { type: "eraseStarted", id: 4, at: 100 });
    expect(p.screen).toBe("eraseProgress");
    expect(p.op?.kind === "erase" && p.op.progress.total).toBe(2);
    const p2 = reducer(p, { type: "opProgress", id: 4, progress: { stage: "erasing", done: 1, total: 2 }, at: 5600 });
    expect(p2.op?.kind === "erase" && p2.op.bankStart).toEqual([100, 5600]);
    const d = reducer(p2, { type: "eraseFinished", id: 4, result });
    expect(d.screen).toBe("eraseDone");
    expect(d.eraseResult).toEqual(result);
    expect(d.quick?.lastWrittenCrc).toBeNull();
  });

  it("cancel in the confirm sheet returns to the device", () => {
    const s = run(...connected, { type: "navigate", to: "eraseConfirm" }, { type: "confirmCancelled" });
    expect(s.screen).toBe("device");
  });

  it("an error in the middle lands on the device with a banner", () => {
    const s = run(
      ...connected,
      { type: "navigate", to: "eraseConfirm" },
      { type: "eraseStarted", id: 5, at: 0 },
      { type: "opFailed", id: 5, error: { kind: "Other", message: "flash operation failed (status 0x00020000)" } },
    );
    expect(s.screen).toBe("device");
    expect(s.banner?.context).toBe("erase");
    expect(s.banner?.actions).toEqual(["retryErase"]);
    expect(s.banner?.message).toMatch(/0x00020000/);
  });

  it("not offered without a flash driver", () => {
    const noDriver = { ...session, target: { ...MOCK_TARGET, geometry: null } };
    const s = run(connected[0], connected[1], { type: "connected", session: noDriver, quick: null });
    expect(reducer(s, { type: "navigate", to: "eraseConfirm" })).toBe(s);
  });
});

describe("overlays", () => {
  it("open and close over any screen", () => {
    const s = run({ type: "overlayOpened", overlay: "settings" });
    expect(s.overlay).toBe("settings");
    expect(s.screen).toBe("empty");
    expect(reducer(s, { type: "overlayClosed" }).overlay).toBeNull();
  });
});
