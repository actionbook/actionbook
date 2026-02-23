/**
 * Check if the Actionbook API is available.
 * Used to skip network-dependent tests when API is unreachable.
 * Matches the Rust integration_test.rs is_api_available() pattern.
 */
export async function isApiAvailable(): Promise<boolean> {
  const apiUrl =
    process.env.ACTIONBOOK_API_URL || "https://api.actionbook.dev";

  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 3000);

    const response = await fetch(`${apiUrl}/health`, {
      signal: controller.signal,
    });
    clearTimeout(timeout);

    if (response.ok) return true;

    // Fallback: try search endpoint (401 means API is up but needs auth)
    const controller2 = new AbortController();
    const timeout2 = setTimeout(() => controller2.abort(), 5000);
    const searchResponse = await fetch(
      `${apiUrl}/api/actions/search?q=test&limit=1`,
      { signal: controller2.signal }
    );
    clearTimeout(timeout2);

    return searchResponse.ok || searchResponse.status === 401;
  } catch {
    return false;
  }
}
