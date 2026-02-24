# Privacy Policy - Actionbook Dify Plugin

## Overview

This policy describes what data the Actionbook Dify Plugin processes when tools are executed in Dify.

## Information Processed

Depending on which tools are used, the plugin may process:

- **Search query text** (for `search_actions`)
- **Area IDs** (for `get_action_by_area_id`)
- **Cloud browser credentials and session data** (for browser tools), including:
  - provider API key (e.g., Hyperbrowser API key)
  - session ID / WebSocket endpoint
  - optional `profile_id` used for browser state persistence
- **Website interaction data** needed to perform browser actions (URL, selectors, typed text, snapshots/HTML/text outputs)

## Third-Party Services

This plugin may communicate with:

- **Actionbook API** (`https://api.actionbook.dev`) for action search and action detail lookup
- **Hyperbrowser API** (when browser session tools are used) for cloud browser session lifecycle
- **Target websites** that the workflow/agent navigates to via browser automation

## Data Storage and Retention

- The plugin code does **not** implement a standalone user database.
- Runtime/session data may exist in Dify runtime context and/or cloud browser provider session state while a workflow is running.
- If a stable `profile_id` is used, browser state (for example cookies/localStorage) can be persisted by the browser provider for reuse across sessions.
- Actionbook service-side logging/retention is controlled by Actionbook service policy.

## Logging

- The plugin includes technical logging for debugging and error handling.
- Logs may include limited operational context (for example query length, area ID, status code, or error category).
- Secrets should not be intentionally printed in plaintext by plugin responses.

## Security

- Network communication uses HTTPS endpoints where supported by upstream services.
- Credentials are handled through Dify plugin credential/tool parameter mechanisms.

## Changes

Updates to this privacy policy will be reflected in plugin releases.

**Last Updated**: 2026-02-24
