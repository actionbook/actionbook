import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary.js";
import { createIsolatedEnv } from "./helpers/config.js";

const binary = getActionbookBinary();
const hasBinary = !!binary;

describe.skipIf(!hasBinary)("global advanced options", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(() => {
    isolatedEnv = createIsolatedEnv();
  });

  // ── --browser-mode option ─────────────────────────────────────────────

  describe("--browser-mode option", () => {
    it("accepts --browser-mode isolated with browser status", async () => {
      const result = await runCli(
        ["--browser-mode", "isolated", "browser", "status"],
        { env: isolatedEnv.env, timeout: 10000 }
      );
      expect(result.exitCode).toBe(0);
    });

    it("accepts --browser-mode extension with browser status", async () => {
      const result = await runCli(
        ["--browser-mode", "extension", "browser", "status"],
        { env: isolatedEnv.env, timeout: 10000 }
      );
      expect(result.exitCode).toBe(0);
    });

    it("rejects invalid --browser-mode value", async () => {
      const result = await runCli(
        ["--browser-mode", "invalid-mode", "browser", "status"],
        { env: isolatedEnv.env, timeout: 5000 }
      );
      expect(result.exitCode).toBe(2);
    });
  });

  // ── stealth options ───────────────────────────────────────────────────

  describe("stealth options", () => {
    it("--help shows --stealth option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--stealth");
    });

    it("--help shows --stealth-os option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--stealth-os");
    });

    it("--help shows --stealth-gpu option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--stealth-gpu");
    });

    it("accepts --stealth flag with browser status", async () => {
      const result = await runCli(
        ["--stealth", "browser", "status"],
        { env: isolatedEnv.env, timeout: 10000 }
      );
      expect(result.exitCode).toBe(0);
    });
  });

  // ── camofox options ───────────────────────────────────────────────────

  describe("camofox options", () => {
    it("--help shows --camofox option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--camofox");
    });

    it("--help shows --camofox-port option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--camofox-port");
    });
  });
});
