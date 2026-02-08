#!/usr/bin/env node
/**
 * Mock extension client for E2E testing the extension bridge.
 *
 * Usage:
 *   1. Start the bridge:  actionbook extension serve
 *   2. Run this script:   node tests/mock-extension.mjs [--port 19222]
 *   3. Send CLI commands:  actionbook --extension browser eval "1+1"
 *
 * The mock extension connects as type "extension" and responds to:
 *   - Extension.ping        -> { pong: true, timestamp }
 *   - Extension.listTabs    -> mock tab list
 *   - Extension.createTab   -> mock tab created
 *   - Extension.activateTab -> success
 *   - Extension.detachTab   -> success
 *   - Runtime.evaluate      -> evaluates simple JS expressions locally
 *   - Page.navigate         -> success with url echo
 *   - Page.reload           -> success
 *   - Page.captureScreenshot -> tiny 1x1 red PNG in base64
 *   - Page.printToPDF       -> minimal PDF in base64
 *   - Network.getCookies    -> empty cookie list
 *   - Network.setCookie     -> success
 *   - Network.deleteCookies -> success
 *   - Network.clearBrowserCookies -> success
 *   - *                     -> generic success with method echo
 */

import { WebSocket } from "ws";

const port = parseInt(process.argv.find((_, i, a) => a[i - 1] === "--port") || "19222", 10);
const url = `ws://127.0.0.1:${port}`;

console.log(`Connecting to bridge at ${url}...`);

const ws = new WebSocket(url);

ws.on("open", () => {
  // Identify as extension
  ws.send(JSON.stringify({ type: "extension" }));
  console.log("Connected as extension. Waiting for commands...\n");
});

ws.on("message", (data) => {
  let msg;
  try {
    msg = JSON.parse(data.toString());
  } catch {
    console.error("Invalid JSON received:", data.toString());
    return;
  }

  const { id, method, params } = msg;
  console.log(`<- [${id}] ${method}`, params ? JSON.stringify(params).substring(0, 120) : "");

  let result;

  switch (method) {
    case "Extension.ping":
      result = { pong: true, timestamp: Date.now() };
      break;

    case "Extension.listTabs":
      result = {
        tabs: [
          { id: 1, title: "Mock Tab 1", url: "https://example.com" },
          { id: 2, title: "Mock Tab 2", url: "https://example.org" },
        ],
      };
      break;

    case "Extension.createTab":
      result = {
        tabId: 99,
        title: "New Tab",
        url: params?.url || "about:blank",
      };
      break;

    case "Extension.activateTab":
      result = { success: true, tabId: params?.tabId };
      break;

    case "Extension.detachTab":
      result = { success: true };
      break;

    case "Runtime.evaluate": {
      const expression = params?.expression || "";
      // Try to evaluate simple expressions
      let value;
      try {
        // Only evaluate safe, simple expressions
        if (/^[\d\s+\-*/().]+$/.test(expression)) {
          value = Function(`"use strict"; return (${expression})`)();
        } else if (expression === "document.title") {
          value = "Mock Page Title";
        } else if (expression === "window.location.href") {
          value = "https://example.com/mock";
        } else if (expression === "document.body.innerText") {
          value = "Mock page text content";
        } else if (expression === "document.documentElement.outerHTML") {
          value = "<html><head><title>Mock</title></head><body>Mock</body></html>";
        } else if (expression.includes("innerWidth")) {
          value = '{"width":1280,"height":720}';
        } else if (expression.includes("readyState")) {
          value = "https://example.com/mock";
        } else {
          // Return a generic success object for complex JS
          value = { success: true, mock: true };
        }
      } catch {
        value = null;
      }
      result = {
        result: {
          type: typeof value,
          value,
        },
      };
      break;
    }

    case "Page.navigate":
      result = {
        frameId: "mock-frame-1",
        loaderId: "mock-loader-1",
        url: params?.url,
      };
      break;

    case "Page.reload":
      result = {};
      break;

    case "Page.captureScreenshot":
      // 1x1 red PNG pixel in base64
      result = {
        data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==",
      };
      break;

    case "Page.printToPDF":
      // Minimal valid PDF in base64
      result = {
        data: "JVBERi0xLjAKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSA+PgplbmRvYmoKeHJlZgowIDQKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDA5IDAwMDAwIG4gCjAwMDAwMDAwNTggMDAwMDAgbiAKMDAwMDAwMDExNSAwMDAwMCBuIAp0cmFpbGVyCjw8IC9TaXplIDQgL1Jvb3QgMSAwIFIgPj4Kc3RhcnR4cmVmCjIwNgolJUVPRgo=",
      };
      break;

    case "Network.getCookies":
      result = { cookies: [] };
      break;

    case "Network.setCookie":
      result = { success: true };
      break;

    case "Network.deleteCookies":
      result = {};
      break;

    case "Network.clearBrowserCookies":
      result = {};
      break;

    default:
      // Generic success for any unhandled method
      result = { success: true, method, mock: true };
      break;
  }

  const response = JSON.stringify({ id, result });
  ws.send(response);
  console.log(`-> [${id}] response sent (${Object.keys(result).join(", ")})`);
});

ws.on("close", () => {
  console.log("\nDisconnected from bridge.");
  process.exit(0);
});

ws.on("error", (err) => {
  console.error("WebSocket error:", err.message);
  console.error("Is the bridge running? Start it with: actionbook extension serve");
  process.exit(1);
});

// Graceful shutdown
process.on("SIGINT", () => {
  console.log("\nShutting down...");
  ws.close();
});
