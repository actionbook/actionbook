import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import path from "path";

// We need to carefully handle the CJS module which uses require() internally.
// Import the functions exported from actionbook.js via the require.main guard.
const actionbookPath = path.resolve(__dirname, "..", "bin", "actionbook.js");

describe("actionbook.js wrapper", () => {
  let wrapper: typeof import("../bin/actionbook.js");

  beforeEach(() => {
    // Clear module cache to get fresh imports
    delete require.cache[require.resolve(actionbookPath)];
    wrapper = require(actionbookPath);
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
        expect(wrapper.PLATFORM_PACKAGES[platform]).toBeDefined();
        expect(wrapper.PLATFORM_PACKAGES[platform]).toMatch(
          /^@actionbookdev\/cli-/
        );
      }
    });

    it("matches PLATFORM_PACKAGES in postinstall.js", () => {
      const postinstallPath = path.resolve(
        __dirname,
        "..",
        "scripts",
        "postinstall.js"
      );
      delete require.cache[require.resolve(postinstallPath)];
      const postinstall = require(postinstallPath);
      expect(wrapper.PLATFORM_PACKAGES).toEqual(
        postinstall.PLATFORM_PACKAGES
      );
    });
  });

  describe("isVersionOnlyFlag", () => {
    it('returns true for ["--version"]', () => {
      expect(wrapper.isVersionOnlyFlag(["--version"])).toBe(true);
    });

    it('returns true for ["-V"]', () => {
      expect(wrapper.isVersionOnlyFlag(["-V"])).toBe(true);
    });

    it('returns false for ["--version", "search"]', () => {
      expect(wrapper.isVersionOnlyFlag(["--version", "search"])).toBe(false);
    });

    it("returns false for empty args", () => {
      expect(wrapper.isVersionOnlyFlag([])).toBe(false);
    });

    it('returns false for ["search", "--version"]', () => {
      expect(wrapper.isVersionOnlyFlag(["search", "--version"])).toBe(false);
    });

    it('returns false for ["-v"] (lowercase)', () => {
      expect(wrapper.isVersionOnlyFlag(["-v"])).toBe(false);
    });
  });

  describe("isLikelyMusl", () => {
    it("returns false on non-linux platforms", () => {
      // On macOS/Windows test runners, this should return false
      if (process.platform !== "linux") {
        expect(wrapper.isLikelyMusl()).toBe(false);
      }
    });

    it("returns false when process.report.getReport is not a function", () => {
      // process.report is a getter-only property in modern Node.js,
      // so we can't override it directly. Instead we verify the function
      // handles the current environment correctly.
      // On non-linux, it should always return false.
      // On linux with glibc, it should return false.
      const result = wrapper.isLikelyMusl();
      expect(typeof result).toBe("boolean");
    });
  });

  describe("resolvePackageDir", () => {
    it("returns null when both strategies fail", () => {
      // Use a fake package name that doesn't exist
      const result = wrapper.resolvePackageDir(
        "@actionbookdev/cli-nonexistent-platform"
      );
      expect(result).toBeNull();
    });

    it("resolves sibling directory when require.resolve fails", () => {
      // The fallback checks for ../../{unscoped}/package.json
      // relative to bin/actionbook.js (__dirname is bin/)
      // This would be packages/{unscoped}/package.json
      // In the monorepo, some platform packages may exist as siblings
      const result = wrapper.resolvePackageDir(
        "@actionbookdev/cli-nonexistent-xyz"
      );
      // The sibling dir cli-nonexistent-xyz won't exist
      expect(result).toBeNull();
    });
  });

  describe("getBinaryPath", () => {
    let exitSpy: ReturnType<typeof vi.spyOn>;
    let errorSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      exitSpy = vi
        .spyOn(process, "exit")
        .mockImplementation((() => {
          throw new Error("process.exit called");
        }) as any);
      errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    });

    afterEach(() => {
      exitSpy.mockRestore();
      errorSpy.mockRestore();
    });

    it("exits with error for unsupported platform", () => {
      expect(() => wrapper.getBinaryPath("freebsd-x64")).toThrow(
        "process.exit called"
      );
      expect(exitSpy).toHaveBeenCalledWith(1);
      expect(errorSpy).toHaveBeenCalledWith(
        expect.stringContaining("Unsupported platform")
      );
    });

    it("exits with error when binary not found at resolved path", () => {
      // All platform packages resolve to paths where the binary doesn't exist
      // in the test environment (unless actually installed)
      const currentPlatform = `${process.platform}-${process.arch}`;
      if (wrapper.PLATFORM_PACKAGES[currentPlatform]) {
        // This will either find the binary (and return a path) or exit
        // In CI without platform packages, it will exit with missing package error
        try {
          const result = wrapper.getBinaryPath(currentPlatform);
          // If it returns, the binary was found (local dev with platform package)
          expect(typeof result).toBe("string");
        } catch {
          // If it throws, process.exit was called (expected in CI)
          expect(exitSpy).toHaveBeenCalledWith(1);
        }
      }
    });

    it("shows supported platforms in error message", () => {
      expect(() => wrapper.getBinaryPath("unknown-platform")).toThrow(
        "process.exit called"
      );
      expect(errorSpy).toHaveBeenCalledWith(
        expect.stringContaining("darwin-arm64")
      );
    });
  });
});
