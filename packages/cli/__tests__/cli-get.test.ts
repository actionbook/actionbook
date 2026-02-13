import { describe, it, expect, beforeAll } from "vitest";
import { getActionbookBinary, runCli } from "./helpers/binary";
import { isApiAvailable } from "./helpers/api";
import { createIsolatedEnv } from "./helpers/config";

const binary = getActionbookBinary();
const hasBinary = !!binary;

let apiAvailable = false;

describe.skipIf(!hasBinary)("get command", () => {
  let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

  beforeAll(async () => {
    isolatedEnv = createIsolatedEnv();
    apiAvailable = await isApiAvailable();
  });

  describe("argument validation", () => {
    it("requires an area_id argument", async () => {
      const result = await runCli(["get"], { env: isolatedEnv.env });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("AREA_ID");
    });

    it("shows help with usage info", async () => {
      const result = await runCli(["get", "--help"], {
        env: isolatedEnv.env,
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Area ID");
    });
  });

  describe.skipIf(!apiAvailable)("with API", () => {
    it("returns error for invalid area_id", async () => {
      const result = await runCli(["get", "invalid-area-id-xyz"], {
        env: isolatedEnv.env,
        timeout: 30000,
      });
      expect(result.exitCode).toBe(1);
    });
  });
});
