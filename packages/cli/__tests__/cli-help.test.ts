import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary.js";
import { createIsolatedEnv } from "./helpers/config.js";

const binary = getActionbookBinary();
const hasBinary = !!binary;

describe.skipIf(!hasBinary)("top-level help", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(() => {
    isolatedEnv = createIsolatedEnv();
  });

  // ── top-level commands ───────────────────────────────────────────────

  describe("--help lists all top-level commands", () => {
    it("includes browser command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("browser");
    });

    it("includes search command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("search");
    });

    it("includes get command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("get");
    });

    it("includes sources command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("sources");
    });

    it("includes config command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("config");
    });

    it("includes profile command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("profile");
    });

    it("includes extension command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("extension");
    });

    it("includes setup command", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("setup");
    });
  });

  // ── global options ───────────────────────────────────────────────────

  describe("--help lists global options", () => {
    it("includes --json option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--json");
    });

    it("includes --verbose option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--verbose");
    });

    it("includes --headless option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--headless");
    });

    it("includes --browser-path option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--browser-path");
    });

    it("includes --cdp option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--cdp");
    });

    it("includes --profile option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--profile");
    });

    it("includes --api-key option", async () => {
      const result = await runCli(["--help"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--api-key");
    });
  });
});
