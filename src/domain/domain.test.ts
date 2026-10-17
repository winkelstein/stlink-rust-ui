import { describe, expect, it } from "vitest";
import { H743_GEOMETRY as g, planWrite, sameSectors, sectorRuns } from "./geometry";
import { crc32 } from "./crc32";
import { parseNumber, writeAddrHint, checkReadRange } from "./validate";
import { estimateWrite, remainingSeconds, writeFraction } from "./estimate";
import { fmtFreq, fmtHex32, fmtInt, fmtRange, fmtSize, fmtUid, fmtVoltage, plural, SECTOR_FORMS } from "./format";
import { DEFAULT_QUICK, needsConfirm, onFileChanged, shouldWriteOnConnect, withChosenFile } from "./quickWrite";
import type { BinFile, FlashPlan } from "../backend/types";

function plan(addr: number, len: number): FlashPlan {
  const r = planWrite(g, addr, len);
  if (!r.ok) throw new Error(r.error);
  return r.plan;
}

describe("planWrite (port of geometry.rs)", () => {
  it("small image, one sector", () => {
    const p = plan(0x0800_0000, 16892);
    expect(p.paddedLen).toBe(16896);
    expect(p.sectors).toEqual([{ bank: 0, index: 0, addr: 0x0800_0000 }]);
  });

  it("spans banks", () => {
    expect(plan(0x080e_0000, 256 * 1024).sectors).toEqual([
      { bank: 0, index: 7, addr: 0x080e_0000 },
      { bank: 1, index: 0, addr: 0x0810_0000 },
    ]);
  });

  it("full flash", () => {
    expect(plan(0x0800_0000, 2 * 1024 * 1024).sectors).toHaveLength(16);
  });

  it("rejects bad input", () => {
    expect(planWrite(g, 0x0800_0010, 32)).toEqual({ ok: false, error: "unaligned" });
    expect(planWrite(g, 0x0700_0000, 32)).toEqual({ ok: false, error: "outside" });
    expect(planWrite(g, 0x0820_0000, 32)).toEqual({ ok: false, error: "outside" });
    expect(planWrite(g, 0x081f_ffe0, 64)).toEqual({ ok: false, error: "doesNotFit" });
    expect(planWrite(g, 0x0800_0000, 0)).toEqual({ ok: false, error: "empty" });
    expect(planWrite(g, 0x0800_0000, 2 * 1024 * 1024 + 1)).toEqual({ ok: false, error: "doesNotFit" });
  });

  it("mockup file: 183 KB at flash start erases sectors 0–1", () => {
    const p = plan(0x0800_0000, 187_392);
    expect(p.sectors.map((s) => s.index)).toEqual([0, 1]);
    expect(sectorRuns(g, p.sectors)).toEqual([
      { bank: 0, first: 0, last: 1, addr: 0x0800_0000, end: 0x0804_0000 },
    ]);
  });

  it("sector runs split by bank", () => {
    const runs = sectorRuns(g, plan(0x080e_0000, 256 * 1024).sectors);
    expect(runs.map((r) => [r.bank, r.first, r.last])).toEqual([
      [0, 7, 7],
      [1, 0, 0],
    ]);
  });

  it("sameSectors", () => {
    const a = plan(0x0800_0000, 1000).sectors;
    expect(sameSectors(a, plan(0x0800_0100, 64).sectors)).toBe(true);
    expect(sameSectors(a, plan(0x0802_0000, 64).sectors)).toBe(false);
    expect(sameSectors(null, a)).toBe(false);
  });
});

describe("crc32", () => {
  it("check value", () => {
    expect(crc32(new TextEncoder().encode("123456789"))).toBe(0xcbf43926);
  });
  it("incremental", () => {
    const d = new TextEncoder().encode("123456789");
    expect(crc32(d.subarray(4), crc32(d.subarray(0, 4)))).toBe(0xcbf43926);
  });
});

describe("parseNumber", () => {
  it.each([
    ["0x08000000", 0x0800_0000],
    ["0x0800_0000", 0x0800_0000],
    [" 134217728 ", 134217728],
    ["256K", 256 * 1024],
    ["1M", 1024 * 1024],
    ["0x200000 · 2 МБ", 0x200000],
  ])("%s", (t, v) => expect(parseNumber(t)).toBe(v));

  it.each(["", "0x", "abc", "08000000h", "0x1FFFFFFFF", "-1"])("rejects %s", (t) =>
    expect(parseNumber(t)).toBeNull(),
  );
});

describe("writeAddrHint", () => {
  it("flash start", () => {
    expect(writeAddrHint(g, "0x08000000", 187_392)).toEqual({
      tone: "ok",
      text: "Начало flash, банк 1 · выровнено по сектору",
    });
  });
  it("bank 2 start", () => {
    expect(writeAddrHint(g, "0x08100000", 1000).text).toBe("Начало банка 2 · выровнено по сектору");
  });
  it("mid sector warns", () => {
    expect(writeAddrHint(g, "0x08020100", 1000).tone).toBe("warn");
  });
  it("errors", () => {
    expect(writeAddrHint(g, "zz", 10).tone).toBe("error");
    expect(writeAddrHint(g, "0x08000010", 10).text).toBe("Адрес должен быть кратен 32 байтам");
    expect(writeAddrHint(g, "0x20000000", 10).text).toMatch(/^Адрес вне flash/);
    expect(writeAddrHint(g, "0x081E0000", 256 * 1024).text).toMatch(/^Файл не помещается/);
  });
  it("without a file only checks the address", () => {
    expect(writeAddrHint(g, "0x081FFFE0", null).tone).not.toBe("error");
  });
});

describe("checkReadRange", () => {
  it("ok and errors", () => {
    expect(checkReadRange("0x08000000", "0x40000")).toEqual({ ok: true, addr: 0x0800_0000, size: 0x40000 });
    expect(checkReadRange("x", "1").ok).toBe(false);
    expect(checkReadRange("0x08000000", "0").ok).toBe(false);
    expect(checkReadRange("0xFFFFFFF0", "0x20").ok).toBe(false);
  });
});

describe("estimate", () => {
  const p = plan(0x0800_0000, 187_392);
  const e = estimateWrite(p, true);
  it("bench speeds", () => {
    expect(e.erasing).toBeCloseTo(1.9);
    expect(e.programming).toBeCloseTo(1.34, 2);
  });
  it("no verify", () => {
    expect(estimateWrite(p, false).verifying).toBe(0);
  });
  it("overall fraction is monotonic across stages", () => {
    const a = writeFraction(e, { stage: "erasing", done: 2, total: 2 });
    const b = writeFraction(e, { stage: "programming", done: 0, total: 100 });
    const c = writeFraction(e, { stage: "verifying", done: 100, total: 100 });
    expect(a).toBeCloseTo(b);
    expect(c).toBeCloseTo(1);
    expect(writeFraction(e, { stage: "reading", done: 1, total: 1 })).toBe(0);
  });
  it("remaining time follows actual speed", () => {
    expect(remainingSeconds(10, 0.5, 0.1)).toBe(5);
    expect(remainingSeconds(10, 0.5, 1)).toBeCloseTo(1);
  });
});

describe("format", () => {
  it("values from the mockups", () => {
    expect(fmtSize(187_392)).toBe("183 КБ");
    expect(fmtSize(256 * 1024)).toBe("256 КБ");
    expect(fmtSize(1.75 * 1024 * 1024)).toBe("1,75 МБ");
    expect(fmtSize(2 * 1024 * 1024)).toBe("2 МБ");
    expect(fmtInt(187_392)).toBe("187 392");
    expect(fmtVoltage(3.28)).toBe("3,28 В");
    expect(fmtFreq(1800)).toBe("1,8 МГц");
    expect(fmtFreq(4000)).toBe("4 МГц");
    expect(fmtFreq(950)).toBe("950 кГц");
    expect(fmtHex32(0x0800_0000)).toBe("0x08000000");
    expect(fmtRange(0x0800_0000, 187_392)).toBe("0x08000000–0x0802DBFF");
    expect(fmtUid("003a00293133510d36383837")).toBe("003A0029 3133510D 36383837");
  });
  it("plural", () => {
    expect([1, 2, 5, 11, 21, 22, 16].map((n) => plural(n, SECTOR_FORMS))).toEqual([
      "сектор",
      "сектора",
      "секторов",
      "секторов",
      "сектор",
      "сектора",
      "секторов",
    ]);
  });
});

describe("quick write rules", () => {
  const file: BinFile = { id: "a", name: "blinky.bin", path: null, size: 1000, modifiedAt: 0, crc32: 1 };
  const p = plan(0x0800_0000, 1000);

  it("confirm unless the same sectors were confirmed", () => {
    expect(needsConfirm(DEFAULT_QUICK, p)).toBe(true);
    expect(needsConfirm({ ...DEFAULT_QUICK, lastConfirmedSectors: p.sectors }, p)).toBe(false);
    expect(needsConfirm({ ...DEFAULT_QUICK, lastConfirmedSectors: plan(0x0802_0000, 10).sectors }, p)).toBe(true);
  });

  it("another file switches write-on-connect off", () => {
    const q = { ...DEFAULT_QUICK, file, onConnect: true };
    expect(withChosenFile(q, { ...file, id: "b" }).onConnect).toBe(true);
    expect(withChosenFile(q, { ...file, id: "c", name: "other.bin" }).onConnect).toBe(false);
  });

  it("write on connect needs a file", () => {
    expect(shouldWriteOnConnect(null)).toBe(false);
    expect(shouldWriteOnConnect({ ...DEFAULT_QUICK, onConnect: true })).toBe(false);
    expect(shouldWriteOnConnect({ ...DEFAULT_QUICK, onConnect: true, file })).toBe(true);
  });

  it("file change: CRC and busy checks", () => {
    const q = { ...DEFAULT_QUICK, file, onFileChange: true, lastWrittenCrc: 1 };
    expect(onFileChanged(q, file, false)).toBe("sameCrc");
    expect(onFileChanged(q, { ...file, crc32: 2 }, true)).toBe("busy");
    expect(onFileChanged(q, { ...file, crc32: 2 }, false)).toBe("write");
    expect(onFileChanged({ ...q, onFileChange: false }, { ...file, crc32: 2 }, false)).toBe("off");
    expect(onFileChanged(q, { ...file, name: "x.bin", crc32: 2 }, false)).toBe("off");
  });
});
