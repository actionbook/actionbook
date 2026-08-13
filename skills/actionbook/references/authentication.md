# Authentication Patterns

Use a dedicated named profile when authentication must survive between runs. The canonical owned-profile flow below installs best-effort cleanup **before** browser acquisition. It cleans exactly once and preserves the status on normal exit or command failure; on SIGINT or SIGTERM, it cleans exactly once and then re-raises the same signal.

## Canonical Owned Authentication Session

<!-- executable-auth-cleanup-smoke:start -->
```bash
#!/usr/bin/env bash
set -Eeuo pipefail

ACTIONBOOK="${ACTIONBOOK:-actionbook}"
SESSION="${SESSION:-myapp-auth}"
PROFILE="${PROFILE:-myapp-auth}"
TAB="${TAB:-t1}"
AUTH_URL="${AUTH_URL:-https://app.example.com/login}"
HEADLESS="${HEADLESS:-false}"

cleanup() {
  "$ACTIONBOOK" browser stop --session "$SESSION" >/dev/null 2>&1 || true
}
trap 'status=$?; trap - EXIT INT TERM; cleanup; exit "$status"' EXIT
trap 'trap - EXIT INT TERM; cleanup; kill -INT "$$"' INT
trap 'trap - EXIT INT TERM; cleanup; kill -TERM "$$"' TERM

"$ACTIONBOOK" browser start \
  --session "$SESSION" \
  --profile "$PROFILE" \
  --headless "$HEADLESS" \
  --open-url "$AUTH_URL"
```
<!-- executable-auth-cleanup-smoke:end -->

All dedicated-profile examples below continue that session and keep this trap installed. For a disposable profile, replace the cleanup body with best-effort `browser close --session "$SESSION"` only when profile deletion is intended.

For a shared session, do not install a stop/close trap. Register task tab IDs before creating them so even a failed open is cleaned up:

<!-- executable-shared-cleanup-smoke:start -->
```bash
ACTIONBOOK="${ACTIONBOOK:-actionbook}"
SESSION=lzero-default
OWNED_TABS=(auth-task-login)
cleanup_shared() {
  for tab in "${OWNED_TABS[@]}"; do
    "$ACTIONBOOK" browser close-tab --session "$SESSION" --tab "$tab" >/dev/null 2>&1 || true
  done
}
trap 'status=$?; trap - EXIT INT TERM; cleanup_shared; exit "$status"' EXIT
trap 'trap - EXIT INT TERM; cleanup_shared; kill -INT "$$"' INT
trap 'trap - EXIT INT TERM; cleanup_shared; kill -TERM "$$"' TERM

"$ACTIONBOOK" browser open "https://app.example.com/login" \
  --session "$SESSION" --tab "${OWNED_TABS[0]}"
```
<!-- executable-shared-cleanup-smoke:end -->

## Basic Login Flow

After running the canonical owned-session setup:

```bash
"$ACTIONBOOK" browser snapshot --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser fill "#email" "$APP_USERNAME" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser fill "#password" "$APP_PASSWORD" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser click "button[type=submit]" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait navigation --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser eval "window.location.href" --session "$SESSION" --tab "$TAB"
```

## Profile-Based Session Persistence

The cleanup trap runs bare `browser stop`, so cookies and local storage remain in `$PROFILE`. A later invocation can use the same canonical setup with a dashboard URL:

```bash
SESSION=myapp-auth PROFILE=myapp-auth \
AUTH_URL=https://app.example.com/dashboard \
./my-auth-script.sh
```

Within that invocation:

```bash
"$ACTIONBOOK" browser text "h1" --session "$SESSION" --tab "$TAB"
```

## OAuth / SSO Flow

Set `AUTH_URL=https://app.example.com/auth/google` before canonical acquisition, then use explicit addressing throughout redirects:

```bash
"$ACTIONBOOK" browser snapshot --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser fill "input[type=email]" "$GOOGLE_EMAIL" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser click "#identifierNext" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait navigation --session "$SESSION" --tab "$TAB"

"$ACTIONBOOK" browser snapshot --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser fill "input[type=password]" "$GOOGLE_PASSWORD" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser click "#passwordNext" --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait navigation --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser eval "window.location.href" --session "$SESSION" --tab "$TAB"
```

## Two-Factor Authentication and Manual Pauses

Keep the canonical trap installed while waiting. A live pause is allowed only with a named owner and resume condition, for example: “task `myapp-auth` owns session `myapp-auth`, tab `t1`; resume when the user confirms MFA completed.” A command failure, normal shell exit, SIGINT, or SIGTERM runs the installed cleanup. If ownership is abandoned while the shell remains live, exit the script to run that cleanup.

```bash
"$ACTIONBOOK" browser wait element ".dashboard-header" \
  --timeout 120000 --session "$SESSION" --tab "$TAB"
```

## Cookie-Based Authentication

Open the target origin via `AUTH_URL` in the canonical setup, then set cookies at session scope and navigate the explicit tab:

```bash
"$ACTIONBOOK" browser cookies set "session_token" "$SESSION_TOKEN" \
  --domain ".example.com" --session "$SESSION"
"$ACTIONBOOK" browser goto "https://app.example.com/dashboard" \
  --session "$SESSION" --tab "$TAB"
"$ACTIONBOOK" browser wait navigation --session "$SESSION" --tab "$TAB"
```

## Token Refresh Handling

```bash
URL=$("$ACTIONBOOK" browser eval "window.location.href" --session "$SESSION" --tab "$TAB")
if [[ "$URL" == *"/login"* ]]; then
  "$ACTIONBOOK" browser snapshot --session "$SESSION" --tab "$TAB"
  "$ACTIONBOOK" browser fill "#email" "$APP_USERNAME" --session "$SESSION" --tab "$TAB"
  "$ACTIONBOOK" browser fill "#password" "$APP_PASSWORD" --session "$SESSION" --tab "$TAB"
  "$ACTIONBOOK" browser click "button[type=submit]" --session "$SESSION" --tab "$TAB"
  "$ACTIONBOOK" browser wait navigation --session "$SESSION" --tab "$TAB"
fi
```

## Security Practices

- Source credentials from environment variables or a secret manager; never inline them.
- Treat the profile directory as password-equivalent data and restrict filesystem access.
- Use `HEADLESS=true` with the canonical setup for CI; the same trap still preserves auth state.
- Clear cookies only when authentication revocation is intended:

  ```bash
  "$ACTIONBOOK" browser cookies clear --session "$SESSION"
  ```

- Destructively close or manually remove a profile only when deletion is explicitly intended.
