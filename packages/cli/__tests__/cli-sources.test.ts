import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary";
import { isApiAvailable } from "./helpers/api";
import { createIsolatedEnv } from "./helpers/config";

const binary = getActionbookBinary();
const hasBinary = !!binary;

let apiAvailable = false;

describe.skipIf(!hasBinary)("sources command", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(async () => {
    isolatedEnv = createIsolatedEnv();
    apiAvailable = await isApiAvailable();
  });

  describe("argument validation", () => {
    it("requires a subcommand", async () => {
      const result = await runCli(["sources"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("subcommand");
    });

    it("sources search requires a query", async () => {
      const result = await runCli(["sources", "search"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("QUERY");
    });
  });

  describe.skipIf(!apiAvailable)("with API", () => {
    it("lists sources", async () => {
      const result = await runCli(["sources", "list"], {
        env: isolatedEnv.env,
        timeout: 30000,
      });
      expect(result.exitCode).toBe(0);
    });

    it("lists sources in JSON format", async () => {
      const result = await runCli(["--json", "sources", "list"], {
        env: isolatedEnv.env,
        timeout: 30000,
      });
      expect(result.exitCode).toBe(0);
      // Verify output is valid JSON
      expect(() => JSON.parse(result.stdout)).not.toThrow();
    });

    it("searches sources by keyword", async () => {
      const result = await runCli(["sources", "search", "airbnb"], {
        env: isolatedEnv.env,
        timeout: 30000,
      });
      expect(result.exitCode).toBe(0);
    });
  });
});
