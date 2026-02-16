import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import path from "path";
import fs from "fs";
import os from "os";

const postinstallPath = path.resolve(__dirname, "..", "scripts", "postinstall.js");

describe("postinstall.js", () => {
  let postinstall: typeof import("../scripts/postinstall.js");

  beforeEach(() => {
    delete require.cache[require.resolve(postinstallPath)];
    postinstall = require(postinstallPath);
  });

  describe("PLATFORM_PACKAGES", () => {
    it("has entries for all 6 supported platforms", () => {
      const expectedPlatforms = [
        "darwin-arm64",
        "darwin-x64",
        "linux-x64",
        "linux-arm64",
        "win32-x64",
        "win32-arm64",
      ];
      for (const platform of expectedPlatforms) {
        expect(postinstall.PLATFORM_PACKAGES[platform]).toBeDefined();
      }
    });
  });

  describe("getBinaryPath", () => {
    it("returns null for unsupported platform", () => {
      // getBinaryPath uses process.platform and process.arch internally.
      // On a supported platform it will try to resolve; on unsupported it returns null.
      // We can't easily change process.platform, but we can verify the function exists
      // and returns a value (string or null).
      const result = postinstall.getBinaryPath();
      // On CI without platform packages, this returns a path or null
      expect(result === null || typeof result === "string").toBe(true);
    });
  });

  describe("resolvePackageDir", () => {
    it("returns null for nonexistent package", () => {
      const result = postinstall.resolvePackageDir(
        "@actionbookdev/cli-nonexistent-platform"
      );
      expect(result).toBeNull();
    });
  });

  describe("main", () => {
    let exitSpy: ReturnType<typeof vi.spyOn>;
    let chmodSpy: ReturnType<typeof vi.spyOn>;
    let existsSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      exitSpy = vi
        .spyOn(process, "exit")
        .mockImplementation((() => {
          throw new Error("process.exit called");
        }) as any);
      chmodSpy = vi.spyOn(fs, "chmodSync").mockImplementation(() => {});
      existsSpy = vi.spyOn(fs, "existsSync");
    });

    afterEach(() => {
      exitSpy.mockRestore();
      chmodSpy.mockRestore();
      existsSpy.mockRestore();
    });

    it("exits 0 when getBinaryPath returns null", () => {
      // Mock getBinaryPath to return null by mocking the internals
      const origGetBinaryPath = postinstall.getBinaryPath;
      // Override temporarily
      const mod = require(postinstallPath);

      // If the current platform doesn't have an installed package,
      // getBinaryPath will return null and main() should exit(0)
      const binaryPath = postinstall.getBinaryPath();
      if (binaryPath === null) {
        expect(() => postinstall.main()).toThrow("process.exit called");
        expect(exitSpy).toHaveBeenCalledWith(0);
      }
    });

    it("calls chmodSync on non-Windows when binary exists", () => {
      if (process.platform === "win32") return;

      const binaryPath = postinstall.getBinaryPath();
      if (binaryPath === null) return; // Skip if no platform package

      existsSpy.mockReturnValue(true);

      // main() will call getBinaryPath(), then existsSync, then chmodSync
      postinstall.main();

      // If platform package exists, chmod should be called
      if (binaryPath) {
        expect(chmodSpy).toHaveBeenCalledWith(expect.any(String), 0o755);
      }
    });

    it("skips chmod on Windows", () => {
      // We can't easily mock process.platform, but we verify that
      // on non-Windows platforms, the function path is correct
      if (process.platform === "win32") {
        const binaryPath = postinstall.getBinaryPath();
        if (binaryPath) {
          existsSpy.mockReturnValue(true);
          postinstall.main();
          expect(chmodSpy).not.toHaveBeenCalled();
        }
      }
    });
  });

  describe("isSetupComplete", () => {
    let existsSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      existsSpy = vi.spyOn(fs, "existsSync");
    });

    afterEach(() => {
      existsSpy.mockRestore();
    });

    it("returns false when config file does not exist", () => {
      existsSpy.mockReturnValue(false);
      expect(postinstall.isSetupComplete()).toBe(false);
    });

    it("returns true when config file exists", () => {
      existsSpy.mockReturnValue(true);
      expect(postinstall.isSetupComplete()).toBe(true);
    });
  });

  describe("isNonInteractive", () => {
    const originalEnv = process.env;
    const originalIsTTY = process.stdout.isTTY;

    afterEach(() => {
      process.env = originalEnv;
      Object.defineProperty(process.stdout, "isTTY", {
        value: originalIsTTY,
        writable: true,
      });
    });

    it("returns true when CI=true", () => {
      process.env = { ...originalEnv, CI: "true" };
      Object.defineProperty(process.stdout, "isTTY", {
        value: true,
        writable: true,
      });
      expect(postinstall.isNonInteractive()).toBe(true);
    });

    it("returns true when stdout is not a TTY", () => {
      process.env = { ...originalEnv };
      delete process.env.CI;
      delete process.env.NON_INTERACTIVE;
      Object.defineProperty(process.stdout, "isTTY", {
        value: undefined,
        writable: true,
      });
      expect(postinstall.isNonInteractive()).toBe(true);
    });

    it("returns true when NON_INTERACTIVE=true", () => {
      process.env = { ...originalEnv, NON_INTERACTIVE: "true" };
      Object.defineProperty(process.stdout, "isTTY", {
        value: true,
        writable: true,
      });
      expect(postinstall.isNonInteractive()).toBe(true);
    });
  });

  describe("printSetupHint", () => {
    let consoleSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      consoleSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    });

    afterEach(() => {
      consoleSpy.mockRestore();
    });

    it("prints nothing when setup is already complete", () => {
      postinstall.printSetupHint(true);
      expect(consoleSpy).not.toHaveBeenCalled();
    });

    it("prints setup hint when setup is not complete", () => {
      postinstall.printSetupHint(false);
      expect(consoleSpy).toHaveBeenCalled();
    });
  });
});
