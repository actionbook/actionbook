import { describe, it, expect, beforeEach } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary.js";
import { createIsolatedEnv } from "./helpers/config.js";

const binary = getActionbookBinary();

describe.skipIf(!binary)("profile command", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeEach(() => {
    isolatedEnv = createIsolatedEnv();
  });

  // ── profile list ───────────────────────────────────────────────────

  describe("profile list", () => {
    it("lists profiles in text format", async () => {
      const result = await runCli(["profile", "list"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Profiles");
    });

    it("lists profiles in JSON format", async () => {
      const result = await runCli(["--json", "profile", "list"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      const json = JSON.parse(result.stdout);
      expect(Array.isArray(json)).toBe(true);
    });
  });

  // ── profile create ─────────────────────────────────────────────────

  describe("profile create", () => {
    it("creates a profile", async () => {
      const result = await runCli(
        ["profile", "create", "test-profile"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Created profile");
      expect(result.stdout).toContain("test-profile");
    });

    it("creates a profile with custom CDP port in JSON format", async () => {
      const result = await runCli(
        [
          "--json",
          "profile",
          "create",
          "test-profile-json",
          "--cdp-port",
          "9333",
        ],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(0);
      const json = JSON.parse(result.stdout);
      expect(json.success).toBe(true);
      expect(json.name).toBe("test-profile-json");
      expect(json.cdp_port).toBe(9333);
    });

    it("auto-assigns CDP port when not specified", async () => {
      const result = await runCli(
        ["--json", "profile", "create", "auto-port-profile"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(0);
      const json = JSON.parse(result.stdout);
      expect(json.success).toBe(true);
      expect(typeof json.cdp_port).toBe("number");
      expect(json.cdp_port).toBeGreaterThan(0);
    });
  });

  // ── profile show ───────────────────────────────────────────────────

  describe("profile show", () => {
    it("shows profile details after creation", async () => {
      // Create first
      await runCli(
        ["profile", "create", "show-test", "--cdp-port", "9444"],
        { env: isolatedEnv.env }
      );

      const result = await runCli(["profile", "show", "show-test"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("show-test");
      expect(result.stdout).toContain("9444");
    });

    it("shows profile details in JSON format", async () => {
      // Create first
      await runCli(
        ["profile", "create", "json-show", "--cdp-port", "9555"],
        { env: isolatedEnv.env }
      );

      const result = await runCli(
        ["--json", "profile", "show", "json-show"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(0);
      const json = JSON.parse(result.stdout);
      expect(json.name).toBe("json-show");
      expect(json.cdp_port).toBe(9555);
      expect(typeof json.headless).toBe("boolean");
    });

    it("fails for nonexistent profile", async () => {
      const result = await runCli(
        ["profile", "show", "nonexistent-profile"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toMatch(/Profile.*not.*found|ProfileNotFound/i);
    });
  });

  // ── profile delete ─────────────────────────────────────────────────

  describe("profile delete", () => {
    it("deletes a created profile", async () => {
      // Create first
      await runCli(
        ["profile", "create", "delete-me"],
        { env: isolatedEnv.env }
      );

      const result = await runCli(
        ["profile", "delete", "delete-me"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Deleted profile");

      // Verify it's gone
      const showResult = await runCli(
        ["profile", "show", "delete-me"],
        { env: isolatedEnv.env }
      );
      expect(showResult.exitCode).toBe(1);
    });
  });

  // ── argument validation ────────────────────────────────────────────

  describe("argument validation", () => {
    it("profile without subcommand fails", async () => {
      const result = await runCli(["profile"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("subcommand");
    });

    it("profile create without name fails", async () => {
      const result = await runCli(["profile", "create"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("NAME");
    });

    it("profile delete without name fails", async () => {
      const result = await runCli(["profile", "delete"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("NAME");
    });
  });
});
