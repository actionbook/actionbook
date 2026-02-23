import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary";
import { isApiAvailable } from "./helpers/api";
import { createIsolatedEnv } from "./helpers/config";

const binary = getActionbookBinary();
const hasBinary = !!binary;

let apiAvailable = false;

describe.skipIf(!hasBinary)("search command", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(async () => {
    isolatedEnv = createIsolatedEnv();
    apiAvailable = await isApiAvailable();
  });

  describe("argument validation", () => {
    it("requires a query argument", async () => {
      const result = await runCli(["search"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("QUERY");
    });

    it("shows help with search options", async () => {
      const result = await runCli(["search", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("--domain");
      expect(result.stdout).toContain("--page");
      expect(result.stdout).toContain("--page-size");
    });
  });

  describe.skipIf(!apiAvailable)("with API", () => {
    it("searches with basic query", async () => {
      const result = await runCli(["search", "airbnb", "--page-size", "1"], {
        env: isolatedEnv.env,
        timeout: 30000,
      });
      expect(result.exitCode).toBe(0);
    });

    it("searches with all filter options", async () => {
      const result = await runCli(
        [
          "search",
          "airbnb",
          "--domain",
          "airbnb.com",
          "--page",
          "1",
          "--page-size",
          "5",
        ],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      expect(result.exitCode).toBe(0);
    });

    it("handles search with no results gracefully", async () => {
      const result = await runCli(
        ["search", "nonexistent-xyz-12345", "--page-size", "1"],
        { env: isolatedEnv.env, timeout: 30000 }
      );
      // Empty results should not be an error
      expect(result.exitCode).toBe(0);
    });
  });
});
