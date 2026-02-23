import { describe, it, expect } from "vitest";
import path from "path";
import { readFileSync } from "fs";
import { runWrapper } from "./helpers/binary.js";

const pkgJsonPath = path.resolve(__dirname, "..", "package.json");
const pkg = JSON.parse(readFileSync(pkgJsonPath, "utf8"));

describe("Node.js wrapper integration", () => {
  describe("version flag", () => {
    it("--version outputs package.json version", async () => {
      const result = await runWrapper(["--version"]);
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toBe(`actionbook ${pkg.version}`);
    });

    it("-V outputs package.json version", async () => {
      const result = await runWrapper(["-V"]);
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toBe(`actionbook ${pkg.version}`);
    });
  });

  describe.skipIf(process.platform === "win32")("ACTIONBOOK_BINARY_PATH", () => {
    it("forwards arguments to mock binary", async () => {
      const mockBinary = path.resolve(
        __dirname,
        "fixtures",
        "mock-binary.sh"
      );
      const result = await runWrapper(["arg1", "arg2", "--flag"], {
        env: { ACTIONBOOK_BINARY_PATH: mockBinary },
      });
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("MOCK_ARGS:arg1 arg2 --flag");
    });

    it("propagates exit code from binary", async () => {
      const mockBinary = path.resolve(
        __dirname,
        "fixtures",
        "mock-binary-fail.sh"
      );
      const result = await runWrapper([], {
        env: { ACTIONBOOK_BINARY_PATH: mockBinary },
      });
      expect(result.exitCode).toBe(42);
    });

    it("shows error when binary path is invalid", async () => {
      const result = await runWrapper([], {
        env: { ACTIONBOOK_BINARY_PATH: "/nonexistent/path/to/binary" },
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("Error executing binary");
    });

    it("captures stderr from failing binary", async () => {
      const mockBinary = path.resolve(
        __dirname,
        "fixtures",
        "mock-binary-error.sh"
      );
      const result = await runWrapper(["some", "args"], {
        env: { ACTIONBOOK_BINARY_PATH: mockBinary },
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("Something went wrong");
    });
  });
});
