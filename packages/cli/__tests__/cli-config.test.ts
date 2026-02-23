import { describe, it, expect, beforeAll, beforeEach } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary.js";
import { createIsolatedEnv } from "./helpers/config.js";

const binary = getActionbookBinary();

describe.skipIf(!binary)("config command", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeEach(() => {
    isolatedEnv = createIsolatedEnv();
  });

  // ── config show ────────────────────────────────────────────────────

  describe("config show", () => {
    it("outputs config in text format", async () => {
      const result = await runCli(["config", "show"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      // Default config should contain API and browser sections
      expect(result.stdout).toContain("api");
      expect(result.stdout).toContain("browser");
    });

    it("outputs valid JSON with --json flag", async () => {
      const result = await runCli(["--json", "config", "show"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      const json = JSON.parse(result.stdout);
      expect(json).toHaveProperty("api");
      expect(json).toHaveProperty("browser");
    });
  });

  // ── config path ────────────────────────────────────────────────────

  describe("config path", () => {
    it("outputs a file path", async () => {
      const result = await runCli(["config", "path"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toContain("actionbook");
      expect(result.stdout.trim()).toContain("config");
    });

    it("outputs JSON with --json flag", async () => {
      const result = await runCli(["--json", "config", "path"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      const json = JSON.parse(result.stdout);
      expect(json).toHaveProperty("path");
      expect(json.path).toContain("actionbook");
    });
  });

  // ── config set + get ───────────────────────────────────────────────

  describe("config set and get", () => {
    it("sets and gets a config value", async () => {
      // Set
      const setResult = await runCli(
        ["config", "set", "api.base_url", "https://custom.example.com"],
        { env: isolatedEnv.env }
      );
      expect(setResult.exitCode).toBe(0);

      // Get
      const getResult = await runCli(["config", "get", "api.base_url"], {
        env: isolatedEnv.env,
      });
      expect(getResult.exitCode).toBe(0);
      expect(getResult.stdout.trim()).toBe("https://custom.example.com");
    });

    it("gets config value in JSON format", async () => {
      // Set first
      await runCli(
        ["config", "set", "api.base_url", "https://json-test.example.com"],
        { env: isolatedEnv.env }
      );

      const result = await runCli(
        ["--json", "config", "get", "api.base_url"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(0);
      const json = JSON.parse(result.stdout);
      expect(json.key).toBe("api.base_url");
      expect(json.value).toBe("https://json-test.example.com");
    });

    it("rejects unknown config key on set", async () => {
      const result = await runCli(
        ["config", "set", "unknown.key", "value"],
        { env: isolatedEnv.env }
      );
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("Unknown config key");
    });

    it("rejects unknown config key on get", async () => {
      const result = await runCli(["config", "get", "unknown.key"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("Unknown config key");
    });
  });

  // ── config argument validation ─────────────────────────────────────

  describe("argument validation", () => {
    it("config without subcommand fails", async () => {
      const result = await runCli(["config"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("subcommand");
    });

    it("config set without key fails", async () => {
      const result = await runCli(["config", "set"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("KEY");
    });

    it("config get without key fails", async () => {
      const result = await runCli(["config", "get"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("KEY");
    });
  });
});
