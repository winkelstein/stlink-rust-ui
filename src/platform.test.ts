import { describe, expect, it } from "vitest";
import { detectPlatform } from "./platform";

describe("platform", () => {
  it("from userAgentData, platform or the user agent", () => {
    expect(detectPlatform({ userAgentData: { platform: "Windows" } })).toBe("windows");
    expect(detectPlatform({ platform: "MacIntel" })).toBe("macos");
    expect(detectPlatform({ platform: "Win32" })).toBe("windows");
    expect(detectPlatform({ platform: "Linux x86_64" })).toBe("linux");
    expect(detectPlatform({ userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/120" })).toBe("windows");
    expect(detectPlatform(undefined)).toBe("linux");
  });
});
