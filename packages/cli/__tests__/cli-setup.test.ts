import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary.js";
import { createIsolatedEnv } from "./helpers/config.js";

const binary = getActionbookBinary();
const hasBinary = !!binary;

describe.skipIf(!hasBinary)("setup command", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(() => {
    isolatedEnv = createIsolatedEnv();
  });

  // ── help output ──────────────────────────────────────────────────────

  describe("setup --help", () => {
    it("shows help with --target option", async () => {
      const result = await runCli(["setup", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--target");
    });

    it("shows help with --non-interactive option", async () => {
      const result = await runCli(["setup", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--non-interactive");
    });

    it("shows help with --api-key option", async () => {
      const result = await runCli(["setup", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--api-key");
    });

    it("shows help with --browser option", async () => {
      const result = await runCli(["setup", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--browser");
    });

    it("shows help with --reset option", async () => {
      const result = await runCli(["setup", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--reset");
    });
  });

  // ── non-interactive mode ─────────────────────────────────────────────

  describe("setup --non-interactive", () => {
    it("runs with --non-interactive --target claude --json and exits with meaningful result", async () => {
      const result = await runCli(
        ["setup", "--non-interactive", "--target", "claude", "--json"],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      // In non-interactive mode it should either succeed or fail with a meaningful error
      // (not hang or crash with unexpected exit code)
      expect([0, 1]).toContain(result.exitCode);
    });

    it("runs with --non-interactive --json (no target) and exits with meaningful result", async () => {
      const result = await runCli(
        ["setup", "--non-interactive", "--json"],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      // Without --target, non-interactive setup may succeed (no target = no install step)
      // or fail gracefully. It should not hang or produce an uncontrolled crash.
      expect([0, 1]).toContain(result.exitCode);
    });
  });

  // ── setup --reset ────────────────────────────────────────────────────

  describe("setup --reset", () => {
    it("runs setup with --reset --non-interactive --json", async () => {
      const result = await runCli(
        ["setup", "--reset", "--non-interactive", "--json"],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      expect([0, 1]).toContain(result.exitCode);
    });
  });

  // ── setup --api-key ──────────────────────────────────────────────────

  describe("setup --api-key", () => {
    it("runs setup with --api-key --non-interactive --json", async () => {
      const result = await runCli(
        ["setup", "--non-interactive", "--api-key", "test-key-12345", "--json"],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      expect([0, 1]).toContain(result.exitCode);
    });
  });

  // ── setup --browser ──────────────────────────────────────────────────

  describe("setup --browser", () => {
    it("runs setup with --browser isolated --non-interactive --json", async () => {
      const result = await runCli(
        ["setup", "--non-interactive", "--browser", "isolated", "--json"],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      expect([0, 1]).toContain(result.exitCode);
    });

    it("runs setup with --browser extension --non-interactive --json", async () => {
      const result = await runCli(
        ["setup", "--non-interactive", "--browser", "extension", "--json"],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      expect([0, 1]).toContain(result.exitCode);
    });
  });
});
