import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary";
import { createIsolatedEnv } from "./helpers/config";

const binary = getActionbookBinary();
const hasBinary = !!binary;

describe.skipIf(!hasBinary)("browser command — Tier 1 (no browser required)", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(() => {
    isolatedEnv = createIsolatedEnv();
  });

  // ── 1A. Missing argument validation (7 tests) ─────────────────────

  describe("argument validation — untested subcommands", () => {
    it("browser select requires SELECTOR", async () => {
      const result = await runCli(["browser", "select"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("SELECTOR");
    });

    it("browser select requires VALUE", async () => {
      const result = await runCli(["browser", "select", "#foo"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("VALUE");
    });

    it("browser hover requires SELECTOR", async () => {
      const result = await runCli(["browser", "hover"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("SELECTOR");
    });

    it("browser focus requires SELECTOR", async () => {
      const result = await runCli(["browser", "focus"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("SELECTOR");
    });

    it("browser press requires KEY", async () => {
      const result = await runCli(["browser", "press"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("KEY");
    });

    it("browser switch requires PAGE_ID", async () => {
      const result = await runCli(["browser", "switch"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("PAGE_ID");
    });

    it("browser wait-nav help shows timeout option", async () => {
      const result = await runCli(["browser", "wait-nav", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("timeout");
    });
  });

  // ── 1B. Help output verification (8 tests) ────────────────────────

  describe("help output — untested subcommands", () => {
    it("browser back --help shows description", async () => {
      const result = await runCli(["browser", "back", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("browser forward --help shows description", async () => {
      const result = await runCli(["browser", "forward", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("browser reload --help shows description", async () => {
      const result = await runCli(["browser", "reload", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("browser pages --help shows description", async () => {
      const result = await runCli(["browser", "pages", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("browser html --help shows optional selector", async () => {
      const result = await runCli(["browser", "html", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/selector/i);
    });

    it("browser text --help shows optional selector", async () => {
      const result = await runCli(["browser", "text", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/selector/i);
    });

    it("browser viewport --help shows description", async () => {
      const result = await runCli(["browser", "viewport", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("browser close --help shows description", async () => {
      const result = await runCli(["browser", "close", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });
  });

  // ── 1C. Default values and flag verification (5 tests) ────────────

  describe("default values and flags", () => {
    it("browser goto --help shows default timeout", async () => {
      const result = await runCli(["browser", "goto", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("[default: 30000]");
    });

    it("browser click --help shows --wait option", async () => {
      const result = await runCli(["browser", "click", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--wait");
    });

    it("browser type --help shows --wait option", async () => {
      const result = await runCli(["browser", "type", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--wait");
    });

    it("browser screenshot --help shows --full-page and default path", async () => {
      const result = await runCli(["browser", "screenshot", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--full-page");
      expect(result.stdout).toContain("screenshot.png");
    });

    it("browser cookies clear --help shows available options", async () => {
      const result = await runCli(
        ["browser", "cookies", "clear", "--help"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(0);
      // The help output should at minimum describe the command
      expect(result.stdout).toMatch(/[Cc]lear|cookies/);
    });
  });

  // ── 1D. browser status output (3 tests) ───────────────────────────

  describe("browser status", () => {
    it("shows browser detection info", async () => {
      const result = await runCli(["browser", "status"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Detected Browsers");
    });

    it("shows session status info", async () => {
      const result = await runCli(["browser", "status"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Session Status");
    });

    it("--verbose browser status runs without error", async () => {
      const result = await runCli(["--verbose", "browser", "status"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      expect(result.exitCode).toBe(0);
    });
  });

  // ── 1E. browser connect error handling (2 tests) ──────────────────

  describe("browser connect errors", () => {
    it("rejects invalid endpoint format", async () => {
      const result = await runCli(["browser", "connect", "not-a-port"], {
        env: isolatedEnv.env,
        timeout: 5000,
      });
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("Invalid endpoint");
    });

    it("fails on unreachable port", async () => {
      const result = await runCli(["browser", "connect", "19999"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      expect(result.exitCode).not.toBe(0);
    });
  });

  // ── 1F. Cross-cutting flag validation (3 tests) ───────────────────

  describe("cross-cutting flags", () => {
    it("--extension + --profile is rejected", async () => {
      const result = await runCli(
        ["--extension", "--profile", "test", "browser", "status"],
        { env: isolatedEnv.env, timeout: 5000 }
      );
      expect(result.exitCode).not.toBe(0);
      // Either clap rejects the unknown flag, or runtime rejects the combination
      expect(result.stderr).toMatch(
        /--profile is not supported in extension mode|unexpected argument|not.*supported/i
      );
    });

    it("browser without subcommand shows error", async () => {
      const result = await runCli(["browser"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("subcommand");
    });

    it("browser cookies --help lists all subcommands", async () => {
      const result = await runCli(["browser", "cookies", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("list");
      expect(result.stdout).toContain("get");
      expect(result.stdout).toContain("set");
      expect(result.stdout).toContain("delete");
      expect(result.stdout).toContain("clear");
    });
  });
});
