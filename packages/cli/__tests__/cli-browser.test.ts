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

  // ── 1A-1b. browser type/fill argument validation ─────────────────

  describe("argument validation — browser type and fill", () => {
    it("browser type requires SELECTOR", async () => {
      const result = await runCli(["browser", "type"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/SELECTOR|required/i);
    });

    it("browser type requires TEXT", async () => {
      const result = await runCli(["browser", "type", "#input"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/TEXT|required/i);
    });

    it("browser fill requires SELECTOR", async () => {
      const result = await runCli(["browser", "fill"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/SELECTOR|required/i);
    });

    it("browser fill requires TEXT", async () => {
      const result = await runCli(["browser", "fill", "#input"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/TEXT|required/i);
    });
  });

  // ── 1A-2. browser inspect argument validation ────────────────────

  describe("argument validation — browser inspect", () => {
    it("browser inspect without arguments should fail", async () => {
      const result = await runCli(["browser", "inspect"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/X|required/i);
    });

    it("browser inspect --help shows X, Y and --desc option", async () => {
      const result = await runCli(["browser", "inspect", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/[Xx]/);
      expect(result.stdout).toMatch(/[Yy]/);
      expect(result.stdout).toContain("--desc");
    });
  });

  // ── 1A-3. browser scroll argument validation ────────────────────

  describe("argument validation — browser scroll", () => {
    it("browser scroll without subcommand should fail", async () => {
      const result = await runCli(["browser", "scroll"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("subcommand");
    });

    it("browser scroll down --help shows description", async () => {
      const result = await runCli(["browser", "scroll", "down", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/[Ss]croll.*down|pixels/i);
    });

    it("browser scroll up --help shows description", async () => {
      const result = await runCli(["browser", "scroll", "up", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/[Ss]croll.*up|pixels/i);
    });

    it("browser scroll bottom --help shows description", async () => {
      const result = await runCli(["browser", "scroll", "bottom", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/[Ss]croll.*bottom|page/i);
    });

    it("browser scroll top --help shows description", async () => {
      const result = await runCli(["browser", "scroll", "top", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/[Ss]croll.*top|page/i);
    });

    it("browser scroll to --help shows --align option", async () => {
      const result = await runCli(["browser", "scroll", "to", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--align");
    });

    it("browser scroll to without selector should fail", async () => {
      const result = await runCli(["browser", "scroll", "to"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/SELECTOR|required/i);
    });

    it("browser scroll --help shows --smooth option", async () => {
      const result = await runCli(["browser", "scroll", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--smooth");
    });
  });

  // ── 1A-4. browser restart help ──────────────────────────────────

  describe("help output — browser restart", () => {
    it("browser restart --help shows description", async () => {
      const result = await runCli(["browser", "restart", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });
  });

  // ── 1A-5. browser cookies argument validation ───────────────────

  describe("argument validation — browser cookies subcommands", () => {
    it("browser cookies get without name should fail", async () => {
      const result = await runCli(["browser", "cookies", "get"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/NAME|required/i);
    });

    it("browser cookies set without arguments should fail", async () => {
      const result = await runCli(["browser", "cookies", "set"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/NAME|required/i);
    });

    it("browser cookies delete without name should fail", async () => {
      const result = await runCli(["browser", "cookies", "delete"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/NAME|required/i);
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

  // ── 1E. browser connect validation (4 tests) ────────────────────

  describe("browser connect validation", () => {
    it("browser connect --help shows endpoint argument", async () => {
      const result = await runCli(["browser", "connect", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/endpoint/i);
    });

    it("browser connect requires ENDPOINT argument", async () => {
      const result = await runCli(["browser", "connect"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toMatch(/ENDPOINT|required/i);
    });

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
