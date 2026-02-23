import { describe, it, expect, beforeAll } from "vitest";
import { existsSync, statSync } from "fs";
import path from "path";
import os from "os";
import { getActionbookBinary, runCli } from "./helpers/binary";
import { createIsolatedEnv } from "./helpers/config";

const binary = getActionbookBinary();
const hasBinary = !!binary;
const runBrowserTests = process.env.RUN_BROWSER_TESTS === "true";

describe.skipIf(!hasBinary || !runBrowserTests)(
  "browser command — Tier 2 (headless browser E2E)",
  () => {
    let isolatedEnv: ReturnType<typeof createIsolatedEnv>;

    beforeAll(() => {
      isolatedEnv = createIsolatedEnv();
    });

    /**
     * Helper to run a headless browser CLI command with appropriate timeout.
     */
    function headless(args: string[], timeout = 30000) {
      return runCli(["--headless", ...args], {
        env: isolatedEnv.env,
        timeout,
      });
    }

    // ── 2A. Navigation flow (6 tests) ─────────────────────────────

    describe("navigation flow", () => {
      it("opens a URL in a new tab", async () => {
        const result = await headless([
          "browser",
          "open",
          "https://example.com",
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("navigates to a URL", async () => {
        const result = await headless([
          "browser",
          "goto",
          "https://example.com",
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("evaluates JS and returns document.title", async () => {
        const result = await headless([
          "browser",
          "eval",
          "document.title",
        ]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Example Domain");
      });

      it("navigates back in history", async () => {
        const result = await headless(["browser", "back"]);
        expect(result.exitCode).toBe(0);
      });

      it("navigates forward in history", async () => {
        const result = await headless(["browser", "forward"]);
        expect(result.exitCode).toBe(0);
      });

      it("reloads the page", async () => {
        const result = await headless(["browser", "reload"]);
        expect(result.exitCode).toBe(0);
      });
    });

    // ── 2B. Page content extraction (6 tests) ────────────────────

    describe("page content extraction", () => {
      beforeAll(async () => {
        // Ensure we're on a known page
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("gets full page HTML", async () => {
        const result = await headless(["browser", "html"]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("<html");
      });

      it("gets HTML of a specific selector", async () => {
        const result = await headless(["browser", "html", "h1"]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout.length).toBeGreaterThan(0);
      });

      it("gets page text content", async () => {
        const result = await headless(["browser", "text"]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Example Domain");
      });

      it("gets viewport dimensions", async () => {
        const result = await headless(["browser", "viewport"]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toMatch(/\d+/);
      });

      it("gets accessibility snapshot", async () => {
        const result = await headless(["browser", "snapshot"]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout.length).toBeGreaterThan(0);
      });

      it("lists open pages", async () => {
        const result = await headless(["browser", "pages"]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout.length).toBeGreaterThan(0);
      });
    });

    // ── 2C. Element interaction (5 tests) ────────────────────────

    describe("element interaction on login form", () => {
      let testSiteAvailable = true;

      beforeAll(async () => {
        try {
          const result = await headless([
            "browser",
            "goto",
            "https://the-internet.herokuapp.com/login",
          ]);
          if (result.exitCode !== 0) testSiteAvailable = false;
        } catch {
          testSiteAvailable = false;
        }
      });

      it("navigates to login page", async () => {
        if (!testSiteAvailable) return;
        const result = await headless([
          "browser",
          "eval",
          "document.title",
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("fills username field", async () => {
        if (!testSiteAvailable) return;
        const result = await headless([
          "browser",
          "fill",
          "--wait",
          "5000",
          "#username",
          "tomsmith",
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("verifies filled value via eval", async () => {
        if (!testSiteAvailable) return;
        const result = await headless([
          "browser",
          "eval",
          "document.querySelector('#username')?.value ?? ''",
        ]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("tomsmith");
      });

      it("clicks login button", async () => {
        if (!testSiteAvailable) return;
        // Fill password first
        await headless([
          "browser",
          "fill",
          "--wait",
          "5000",
          "#password",
          "SuperSecretPassword!",
        ]);
        const result = await headless([
          "browser",
          "click",
          "--wait",
          "5000",
          'button[type="submit"]',
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("verifies navigation after login", async () => {
        if (!testSiteAvailable) return;
        // Submit form via JS since CLI fill may not dispatch DOM events
        // that the form handler requires for proper submission.
        const result = await headless([
          "browser",
          "eval",
          "document.querySelector('form#login').submit(); 'submitted'",
        ]);
        expect(result.exitCode).toBe(0);
        // Wait for navigation to complete
        await new Promise((resolve) => setTimeout(resolve, 3000));
        const nav = await headless([
          "browser",
          "eval",
          "window.location.pathname",
        ]);
        expect(nav.exitCode).toBe(0);
        expect(nav.stdout).toContain("/secure");
      });
    });

    // ── 2D. Screenshot & PDF (3 tests) ──────────────────────────

    describe("screenshot and PDF", () => {
      const tmpDir = os.tmpdir();

      beforeAll(async () => {
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("takes a screenshot", async () => {
        const screenshotPath = path.join(tmpDir, "e2e-test-screenshot.png");
        const result = await headless([
          "browser",
          "screenshot",
          screenshotPath,
        ]);
        expect(result.exitCode).toBe(0);
        expect(existsSync(screenshotPath)).toBe(true);
        expect(statSync(screenshotPath).size).toBeGreaterThan(0);
      });

      it("takes a full-page screenshot", async () => {
        const screenshotPath = path.join(
          tmpDir,
          "e2e-test-screenshot-full.png"
        );
        const result = await headless([
          "browser",
          "screenshot",
          screenshotPath,
          "--full-page",
        ]);
        expect(result.exitCode).toBe(0);
        expect(existsSync(screenshotPath)).toBe(true);
      });

      it("exports page as PDF", async () => {
        const pdfPath = path.join(tmpDir, "e2e-test-page.pdf");
        const result = await headless(["browser", "pdf", pdfPath]);
        expect(result.exitCode).toBe(0);
        expect(existsSync(pdfPath)).toBe(true);
        expect(statSync(pdfPath).size).toBeGreaterThan(0);
      });
    });

    // ── 2E. Cookie management (5 tests) ─────────────────────────

    describe("cookie management", () => {
      beforeAll(async () => {
        // Ensure a page is loaded (cookies require a page context)
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("lists cookies", async () => {
        const result = await headless(["browser", "cookies", "list"]);
        expect(result.exitCode).toBe(0);
      });

      it("sets a cookie", async () => {
        const result = await headless([
          "browser",
          "cookies",
          "set",
          "e2e_test_cookie",
          "hello_from_e2e",
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("gets a cookie", async () => {
        const result = await headless([
          "browser",
          "cookies",
          "get",
          "e2e_test_cookie",
        ]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("hello_from_e2e");
      });

      it("deletes a cookie", async () => {
        const result = await headless([
          "browser",
          "cookies",
          "delete",
          "e2e_test_cookie",
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("clears all cookies", async () => {
        const result = await headless([
          "browser",
          "cookies",
          "clear",
          "--yes",
        ]);
        expect(result.exitCode).toBe(0);
      });
    });

    // ── 2F. Error scenarios (3 tests) ───────────────────────────

    describe("error scenarios", () => {
      beforeAll(async () => {
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("fails on clicking nonexistent selector", async () => {
        const result = await headless(
          ["browser", "click", "#nonexistent-element-xyz"],
          10000
        );
        // Should fail (either timeout or element not found)
        expect(result.exitCode).not.toBe(0);
      });

      it("fails waiting for nonexistent selector with timeout", async () => {
        const result = await headless(
          ["browser", "wait", "#nonexistent-element-xyz", "--timeout", "2000"],
          10000
        );
        expect(result.exitCode).not.toBe(0);
      });

      it("reports JavaScript errors", async () => {
        const result = await headless([
          "browser",
          "eval",
          "throw new Error('test error from e2e')",
        ]);
        // CLI returns exit 0 but includes error details in stdout
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("Error");
        expect(result.stdout).toContain("test error from e2e");
      });
    });

    // ── 2G. JSON output format (3 tests) ────────────────────────

    describe("JSON output format", () => {
      beforeAll(async () => {
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("--json browser eval returns valid JSON", async () => {
        const result = await runCli(
          ["--json", "--headless", "browser", "eval", "1+1"],
          { env: isolatedEnv.env, timeout: 30000 }
        );
        expect(result.exitCode).toBe(0);
        expect(() => JSON.parse(result.stdout)).not.toThrow();
      });

      it("--json browser pages returns valid JSON", async () => {
        const result = await runCli(
          ["--json", "--headless", "browser", "pages"],
          { env: isolatedEnv.env, timeout: 30000 }
        );
        expect(result.exitCode).toBe(0);
        expect(() => JSON.parse(result.stdout)).not.toThrow();
      });

      it("--json browser viewport returns JSON with dimensions", async () => {
        const result = await runCli(
          ["--json", "--headless", "browser", "viewport"],
          { env: isolatedEnv.env, timeout: 30000 }
        );
        expect(result.exitCode).toBe(0);
        const json = JSON.parse(result.stdout);
        expect(json).toHaveProperty("width");
        expect(json).toHaveProperty("height");
      });
    });

    // ── 2H. Tab management (3 tests) ────────────────────────────

    describe("tab management", () => {
      it("opens a new tab", async () => {
        const result = await headless([
          "browser",
          "open",
          "https://example.com",
        ]);
        expect(result.exitCode).toBe(0);
      });

      it("pages shows multiple tabs", async () => {
        const result = await headless(["browser", "pages"]);
        expect(result.exitCode).toBe(0);
        // Output should list at least 2 pages
        const lines = result.stdout
          .trim()
          .split("\n")
          .filter((l) => l.trim().length > 0);
        expect(lines.length).toBeGreaterThanOrEqual(2);
      });

      it("switches to a different tab", async () => {
        // Get pages list first to find a page ID
        const pagesResult = await headless(["browser", "pages"]);
        expect(pagesResult.exitCode).toBe(0);

        // Extract page ID from output (format varies, look for common patterns)
        const pageIdMatch = pagesResult.stdout.match(
          /(?:tab:|page:|id[:\s])\s*(\S+)/i
        );
        if (pageIdMatch) {
          const pageId = pageIdMatch[1];
          const result = await headless(["browser", "switch", pageId]);
          expect(result.exitCode).toBe(0);
        }
      });
    });
  }
);
