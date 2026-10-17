import { describe, expect, it } from "vitest";
import { initialState, reducer, stepOf, type Action, type AppState, type PendingWrite } from "./machine";
import { MOCK_PROBE, MOCK_TARGET, mockConnection } from "../backend/mock/mockData";
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
  { type: "probeConnected", probe: MOCK_PROBE },
  { type: "identifyStarted", opts: initialState.connectOptions },
  { type: "identified", session, quick: { ...DEFAULT_QUICK, file, onConnect: true } },
];

describe("hot-plug", () => {
  it("connect shows the device screen, identify fills the session", () => {
    const s1 = run(connected[0]);
    expect(s1.screen).toBe("device");
    expect(s1.session).toBeNull();
    const s = run(...connected);
    expect(s.identifying).toBe(false);
    expect(s.session).toEqual(session);
    expect(s.writeForm.file).toEqual(file);
  });

  it("disconnect returns to empty and keeps write-on-connect armed", () => {
    const s = run(...connected, { type: "probeDisconnected", probeId: MOCK_PROBE.id });
    expect(s.screen).toBe("empty");
    expect(s.session).toBeNull();
    expect(s.armed?.uid).toBe(MOCK_TARGET.uid);
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

  it("identify failure shows CoreNotAccessible hints", () => {
    const s = run(connected[0], connected[1], {
      type: "identifyFailed",
      error: { kind: "CoreNotAccessible", message: "x" },
    });
    expect(s.screen).toBe("device");
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
    expect(s.banner?.text).toMatch(/частично записан/);
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

  it("choosing another file disarms write-on-connect", () => {
    const s = run(...connected, { type: "fileChosen", file: { ...file, id: "f2", name: "other.bin" } });
    expect(s.quick?.onConnect).toBe(false);
    expect(s.screen).toBe("write");
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
