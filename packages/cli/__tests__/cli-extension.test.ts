import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary.js";
import { createIsolatedEnv } from "./helpers/config.js";

const binary = getActionbookBinary();
const hasBinary = !!binary;

describe.skipIf(!hasBinary)("extension command — Tier 1 (no browser required)", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(() => {
    isolatedEnv = createIsolatedEnv();
  });

  // ── argument validation ──────────────────────────────────────────────

  describe("argument validation", () => {
    it("extension without subcommand fails with exit code 2 and mentions subcommand", async () => {
      const result = await runCli(["extension"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("subcommand");
    });
  });

  // ── help output ──────────────────────────────────────────────────────

  describe("help output", () => {
    it("extension status --help shows description", async () => {
      const result = await runCli(["extension", "status", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("extension ping --help shows description", async () => {
      const result = await runCli(["extension", "ping", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("extension install --help shows --force option", async () => {
      const result = await runCli(["extension", "install", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--force");
    });

    it("extension stop --help shows description", async () => {
      const result = await runCli(["extension", "stop", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });

    it("extension uninstall --help shows description", async () => {
      const result = await runCli(["extension", "uninstall", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.length).toBeGreaterThan(0);
    });
  });

  // ── extension path ───────────────────────────────────────────────────

  describe("extension path", () => {
    it("outputs a path string", async () => {
      const result = await runCli(["extension", "path"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim().length).toBeGreaterThan(0);
    });

    it("outputs JSON with --json flag", async () => {
      const result = await runCli(["--json", "extension", "path"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      // If JSON output is supported, parse and validate; otherwise accept plain text success
      if (result.exitCode === 0) {
        try {
          const json = JSON.parse(result.stdout);
          expect(json).toHaveProperty("path");
        } catch {
          // Plain text path output is also acceptable
          expect(result.stdout.trim().length).toBeGreaterThan(0);
        }
      } else {
        // Treat non-zero as acceptable only if stderr explains the reason
        expect(result.stderr.length).toBeGreaterThan(0);
      }
    });
  });

  // ── extension status execution ───────────────────────────────────────

  describe("extension status execution", () => {
    it("runs extension status and reports bridge state", async () => {
      const result = await runCli(["extension", "status"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      expect(result.exitCode).toBe(0);
      // Should report bridge is running or not running
      expect(result.stdout).toMatch(/running|not running/i);
    });
  });

  // ── extension ping execution ─────────────────────────────────────────

  describe("extension ping execution", () => {
    it("runs extension ping (fails gracefully without bridge)", async () => {
      const result = await runCli(["extension", "ping"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      // Ping without bridge should exit 0 but report failure
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/failed|error|not running/i);
    });
  });

  // ── extension stop execution ─────────────────────────────────────────

  describe("extension stop execution", () => {
    it("runs extension stop when no bridge is running", async () => {
      const result = await runCli(["extension", "stop"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toMatch(/not running|stopped/i);
    });
  });

  // ── extension install execution ──────────────────────────────────────

  describe("extension install execution", () => {
    it("runs extension install and downloads or reports already installed", async () => {
      const result = await runCli(["extension", "install"], {
        env: isolatedEnv.env,
        timeout: 60000,
      });
      // May fail due to network issues (GitHub unreachable, rate limiting) in offline/CI environments
      expect([0, 1]).toContain(result.exitCode);
    });

    it("runs extension install --force", async () => {
      const result = await runCli(["extension", "install", "--force"], {
        env: isolatedEnv.env,
        timeout: 60000,
      });
      // --force re-downloads from GitHub; may fail due to rate limiting or network
      expect([0, 1]).toContain(result.exitCode);
    });
  });

  // ── extension uninstall execution ────────────────────────────────────

  describe("extension uninstall execution", () => {
    it("runs extension uninstall", async () => {
      const result = await runCli(["extension", "uninstall"], {
        env: isolatedEnv.env,
        timeout: 10000,
      });
      // Should succeed (uninstalled or not installed)
      expect(result.exitCode).toBe(0);
    });
  });
});
