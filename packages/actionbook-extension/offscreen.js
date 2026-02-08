// Actionbook Offscreen Document - Keeps the Service Worker alive
// Sends periodic pings to background.js to prevent the 30s idle timeout

const KEEPALIVE_INTERVAL_MS = 20000; // 20 seconds (under the 30s SW timeout)

setInterval(() => {
  chrome.runtime.sendMessage({ type: "keepalive" }).catch(() => {
    // SW might be briefly unavailable during restart, ignore
  });
}, KEEPALIVE_INTERVAL_MS);

// Send initial keepalive immediately
chrome.runtime.sendMessage({ type: "keepalive" }).catch(() => {});
