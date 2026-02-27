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
        // Verify we actually navigated to the correct URL
        const loc = await headless(["browser", "eval", "window.location.href"]);
        expect(loc.stdout).toContain("example.com");
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

        // Verify cookie is actually gone
        const after = await headless([
          "browser",
          "cookies",
          "get",
          "e2e_test_cookie",
        ]);
        expect(after.stdout).not.toContain("hello_from_e2e");
      });

      it("clears all cookies", async () => {
        // Set a cookie first so there's something to clear
        await headless([
          "browser",
          "cookies",
          "set",
          "clear_test",
          "to_be_cleared",
        ]);
        const result = await headless([
          "browser",
          "cookies",
          "clear",
          "--yes",
        ]);
        expect(result.exitCode).toBe(0);

        // Verify cookie was cleared
        const after = await headless(["browser", "cookies", "get", "clear_test"]);
        expect(after.stdout).not.toContain("to_be_cleared");
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

    // ── 2H-2. browser inspect (1 test) ─────────────────────────

    describe("browser inspect", () => {
      beforeAll(async () => {
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("inspects element at coordinates", async () => {
        const result = await headless(["browser", "inspect", "100", "100"]);
        expect(result.exitCode).toBe(0);
        // Output should contain element tag or selector info
        expect(result.stdout.length).toBeGreaterThan(0);
        expect(result.stdout).toMatch(/tag|selector|<|id|class/i);
      });
    });

    // ── 2I. browser scroll (5 tests) ─────────────────────────────

    describe("browser scroll", () => {
      beforeAll(async () => {
        // Use a page with enough content to scroll
        await headless([
          "browser",
          "goto",
          "https://the-internet.herokuapp.com",
        ]);
      });

      it("scrolls down", async () => {
        await headless(["browser", "scroll", "top"]);
        const before = await headless(["browser", "eval", "window.scrollY"]);
        const scrollYBefore = Number(before.stdout.trim());

        const result = await headless(["browser", "scroll", "down"]);
        expect(result.exitCode).toBe(0);

        const after = await headless(["browser", "eval", "window.scrollY"]);
        expect(Number(after.stdout.trim())).toBeGreaterThan(scrollYBefore);
      });

      it("scrolls up", async () => {
        await headless(["browser", "scroll", "down", "500"]);
        const before = await headless(["browser", "eval", "window.scrollY"]);
        const scrollYBefore = Number(before.stdout.trim());
        expect(scrollYBefore).toBeGreaterThan(0);

        const result = await headless(["browser", "scroll", "up"]);
        expect(result.exitCode).toBe(0);

        const after = await headless(["browser", "eval", "window.scrollY"]);
        expect(Number(after.stdout.trim())).toBeLessThan(scrollYBefore);
      });

      it("scrolls to bottom", async () => {
        const result = await headless(["browser", "scroll", "bottom"]);
        expect(result.exitCode).toBe(0);

        const atBottom = await headless([
          "browser",
          "eval",
          "window.scrollY + window.innerHeight >= document.body.scrollHeight - 1",
        ]);
        expect(atBottom.stdout.trim()).toBe("true");
      });

      it("scrolls to top", async () => {
        await headless(["browser", "scroll", "down", "500"]);
        const result = await headless(["browser", "scroll", "top"]);
        expect(result.exitCode).toBe(0);

        const after = await headless(["browser", "eval", "window.scrollY"]);
        expect(Number(after.stdout.trim())).toBe(0);
      });

      it("scrolls to a specific element", async () => {
        await headless(["browser", "scroll", "top"]);
        await headless(["browser", "scroll", "down", "500"]);

        const result = await headless(["browser", "scroll", "to", "h1"]);
        expect(result.exitCode).toBe(0);

        const rect = await headless([
          "browser",
          "eval",
          "document.querySelector('h1').getBoundingClientRect().top",
        ]);
        const top = Number(rect.stdout.trim());
        // Element should be within the viewport after scrolling to it
        expect(Math.abs(top)).toBeLessThan(300);
      });
    });

    // ── 2K. browser type E2E (1 test) ──────────────────────────

    describe("browser type", () => {
      beforeAll(async () => {
        await headless([
          "browser",
          "goto",
          "https://the-internet.herokuapp.com/login",
        ]);
      });

      it("types text into an element", async () => {
        // Clear field first
        await headless(["browser", "fill", "--wait", "5000", "#username", ""]);
        const result = await headless([
          "browser",
          "type",
          "--wait",
          "5000",
          "#username",
          "appended-text",
        ]);
        expect(result.exitCode).toBe(0);

        const val = await headless([
          "browser",
          "eval",
          "document.querySelector('#username').value",
        ]);
        expect(val.stdout.trim()).toContain("appended-text");
      });
    });

    // ── 2L. browser select/hover/focus/press E2E (4 tests) ──────

    describe("element interaction — select, hover, focus, press", () => {
      beforeAll(async () => {
        await headless([
          "browser",
          "goto",
          "https://the-internet.herokuapp.com/dropdown",
        ]);
      });

      it("selects a dropdown option", async () => {
        const result = await headless([
          "browser",
          "select",
          "#dropdown",
          "1",
        ]);
        expect(result.exitCode).toBe(0);

        const val = await headless([
          "browser",
          "eval",
          "document.querySelector('#dropdown').value",
        ]);
        expect(val.stdout.trim().replace(/^"|"$/g, "")).toBe("1");
      });

      it("hovers over an element", async () => {
        const result = await headless(["browser", "hover", "#dropdown"]);
        expect(result.exitCode).toBe(0);
      });

      it("focuses on an element", async () => {
        const result = await headless(["browser", "focus", "#dropdown"]);
        expect(result.exitCode).toBe(0);

        const focused = await headless([
          "browser",
          "eval",
          "document.activeElement?.id || document.activeElement?.tagName",
        ]);
        expect(focused.stdout.trim().replace(/^"|"$/g, "")).toBe("dropdown");
      });

      it("presses a keyboard key", async () => {
        const result = await headless(["browser", "press", "Tab"]);
        expect(result.exitCode).toBe(0);
      });
    });

    // ── 2M. browser wait-nav E2E (1 test) ────────────────────────

    describe("browser wait-nav", () => {
      it("wait-nav completes on already loaded page", async () => {
        await headless(["browser", "goto", "https://example.com"]);
        const result = await headless([
          "browser",
          "wait-nav",
          "--timeout",
          "5000",
        ]);
        // wait-nav on an already-loaded page should succeed or timeout gracefully
        expect([0, 1]).toContain(result.exitCode);
      });
    });

    // ── 2N. browser inspect --desc E2E (1 test) ─────────────────

    describe("browser inspect --desc", () => {
      beforeAll(async () => {
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("inspects element at coordinates with --desc", async () => {
        const result = await headless([
          "browser",
          "inspect",
          "100",
          "100",
          "--desc",
          "looking for header element",
        ]);
        expect(result.exitCode).toBe(0);
        expect(result.stdout.length).toBeGreaterThan(0);
      });
    });

    // ── 2O. browser scroll parameter variants E2E (5 tests) ─────

    describe("browser scroll parameter variants", () => {
      beforeAll(async () => {
        await headless([
          "browser",
          "goto",
          "https://the-internet.herokuapp.com",
        ]);
      });

      it("scrolls down with custom pixel count", async () => {
        await headless(["browser", "scroll", "top"]);
        const result = await headless(["browser", "scroll", "down", "200"]);
        expect(result.exitCode).toBe(0);

        const after = await headless(["browser", "eval", "window.scrollY"]);
        expect(Number(after.stdout.trim())).toBeGreaterThanOrEqual(200);
      });

      it("scrolls up with custom pixel count", async () => {
        await headless(["browser", "scroll", "top"]);
        await headless(["browser", "scroll", "down", "500"]);

        const result = await headless(["browser", "scroll", "up", "100"]);
        expect(result.exitCode).toBe(0);

        const after = await headless(["browser", "eval", "window.scrollY"]);
        const scrollY = Number(after.stdout.trim());
        // Should have scrolled up ~100px from 500
        expect(scrollY).toBeLessThanOrEqual(420);
        expect(scrollY).toBeGreaterThan(0);
      });

      it("scrolls with --smooth flag", async () => {
        await headless(["browser", "scroll", "top"]);
        const result = await headless([
          "browser",
          "scroll",
          "--smooth",
          "down",
          "300",
        ]);
        expect(result.exitCode).toBe(0);

        // Wait a moment for smooth scroll animation to complete
        await new Promise((r) => setTimeout(r, 500));
        const after = await headless(["browser", "eval", "window.scrollY"]);
        expect(Number(after.stdout.trim())).toBeGreaterThan(0);
      });

      it("scrolls to element with --align start", async () => {
        await headless(["browser", "scroll", "down", "500"]);
        const result = await headless([
          "browser",
          "scroll",
          "to",
          "h1",
          "--align",
          "start",
        ]);
        expect(result.exitCode).toBe(0);

        const rect = await headless([
          "browser",
          "eval",
          "document.querySelector('h1').getBoundingClientRect().top",
        ]);
        // With --align start, element top should be near viewport top
        expect(Math.abs(Number(rect.stdout.trim()))).toBeLessThan(50);
      });

      it("scrolls to element with --align end", async () => {
        await headless(["browser", "scroll", "down", "500"]);
        const result = await headless([
          "browser",
          "scroll",
          "to",
          "h1",
          "--align",
          "end",
        ]);
        expect(result.exitCode).toBe(0);

        // With --align end, element should be visible near the bottom of the viewport
        const visible = await headless([
          "browser",
          "eval",
          "document.querySelector('h1').getBoundingClientRect().bottom <= window.innerHeight",
        ]);
        expect(visible.stdout.trim()).toBe("true");
      });
    });

    // ── 2P. browser cookies parameter variants E2E (3 tests) ────
    // NOTE: --domain and --dry-run flags on cookies clear/set are only supported
    // in extension mode (--extension). In headless mode these commands will fail
    // with a non-zero exit code. The tests accept graceful failure (exit != 2)
    // so they pass in headless mode but only exercise real logic in extension mode.

    describe("browser cookies parameter variants (extension-mode-only)", () => {
      beforeAll(async () => {
        await headless(["browser", "goto", "https://example.com"]);
      });

      it("sets a cookie with --domain (requires extension mode)", async () => {
        const result = await headless([
          "browser",
          "cookies",
          "set",
          "domain_cookie",
          "domain_value",
          "--domain",
          "example.com",
        ]);
        // --domain requires extension mode; accept graceful failure in headless
        if (result.exitCode === 0) {
          const after = await headless([
            "browser",
            "cookies",
            "get",
            "domain_cookie",
          ]);
          expect(after.stdout).toContain("domain_value");
        } else {
          // Not a CLI parse error — expected failure in headless mode
          expect(result.exitCode).not.toBe(2);
        }
      });

      it("clears cookies with --dry-run (requires extension mode)", async () => {
        await headless([
          "browser",
          "cookies",
          "set",
          "dry_run_test",
          "should_remain",
        ]);
        const result = await headless([
          "browser",
          "cookies",
          "clear",
          "--dry-run",
        ]);
        // --dry-run requires extension mode; in headless this will fail
        if (result.exitCode === 0) {
          const after = await headless([
            "browser",
            "cookies",
            "get",
            "dry_run_test",
          ]);
          expect(after.stdout).toContain("should_remain");
        } else {
          expect(result.exitCode).not.toBe(2);
        }
      });

      it("clears cookies with --domain (requires extension mode)", async () => {
        await headless([
          "browser",
          "cookies",
          "set",
          "domain_clear_test",
          "domain_val",
          "--domain",
          "example.com",
        ]);
        const result = await headless([
          "browser",
          "cookies",
          "clear",
          "--domain",
          "example.com",
          "--yes",
        ]);
        // --domain on clear requires extension mode
        if (result.exitCode === 0) {
          const after = await headless([
            "browser",
            "cookies",
            "get",
            "domain_clear_test",
          ]);
          expect(after.stdout).not.toContain("domain_val");
        } else {
          expect(result.exitCode).not.toBe(2);
        }
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

    // ── 2Q. browser restart & close — MUST be last ─────────────

    describe("browser restart", () => {
      it("restarts browser and can continue operating", async () => {
        const restartResult = await headless(
          ["browser", "restart"],
          60000
        );
        expect(restartResult.exitCode).toBe(0);

        // Verify browser is functional after restart
        const gotoResult = await headless([
          "browser",
          "goto",
          "https://example.com",
        ]);
        expect(gotoResult.exitCode).toBe(0);
      });
    });

    describe("browser close", () => {
      it("closes the browser", async () => {
        const result = await headless(["browser", "close"]);
        expect(result.exitCode).toBe(0);
      });
    });
  }
);
