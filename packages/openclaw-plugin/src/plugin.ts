/**
 * Actionbook OpenClaw Plugin
 *
 * Registers search_actions and get_action_by_area_id as native OpenClaw agent tools.
 * Provides pre-verified selectors for token-efficient browser automation.
 */

import { Type } from "@sinclair/typebox";
import type { OpenClawPluginApi } from "openclaw/plugin-sdk/core";
import { isActionbookError } from "@actionbookdev/sdk";
import { ApiClient } from "./lib/api-client.js";

const DEFAULT_API_URL = "https://api.actionbook.dev";
const ACTIONBOOK_GUIDANCE = [
  "## Actionbook Browser Automation",
  "When the user needs to interact with any website, call `search_actions` BEFORE taking page snapshots. Use `get_action_by_area_id` to get pre-verified selectors from the results.",
].join("\n");
const SEARCH_ACTIONS_DESCRIPTION = `Search for website action manuals by keyword.

Use this tool to find actions, page elements, and their selectors for browser automation.
Returns area_id identifiers with descriptions and health scores.

Example queries:
- "airbnb search" → find Airbnb search-related actions
- "google login" → find Google login actions

Typical workflow:
1. search_actions({ query: "airbnb search" })
2. Get area_id from results (e.g., "airbnb.com:/:default")
3. get_action_by_area_id({ area_id: "airbnb.com:/:default" })
4. Use returned selectors with browser tools`;
const GET_ACTION_BY_AREA_ID_DESCRIPTION = `Get complete action details by area_id, including DOM selectors.

Area ID format: site:path:area (e.g., "airbnb.com:/:default")

Returns:
- Page description and functions
- Interactive elements with selectors (CSS, XPath, data-testid, role, aria-label)
- Element types and allowed methods (click, type, etc.)
- Health score indicating selector reliability

Use returned selectors with browser automation tools:
- data-testid selectors (0.95 confidence) → use with browser eval
- aria-label selectors (0.88 confidence) → use with browser eval
- role selectors (0.9 confidence) → use with browser snapshot + click`;

type ActionbookPluginConfig = {
  apiKey?: string;
  apiUrl?: string;
};

const PRIVATE_IP_RANGES = [
  /^127\./,
  /^10\./,
  /^172\.(1[6-9]|2\d|3[01])\./,
  /^192\.168\./,
  /^169\.254\./,
  /^0\./,
  /^::1$/,
  /^fe80:/i,
  /^fc00:/i,
  /^fd00:/i,
];

function isPrivateHost(hostname: string): boolean {
  if (
    hostname === "localhost" ||
    hostname === "[::1]" ||
    hostname.endsWith(".local")
  ) {
    return true;
  }
  // URL.hostname wraps IPv6 in brackets (e.g. "[fd00::1]") — strip them
  const bare = hostname.startsWith("[") && hostname.endsWith("]")
    ? hostname.slice(1, -1)
    : hostname;
  return PRIVATE_IP_RANGES.some((re) => re.test(bare));
}

function resolveApiUrl(value: unknown): string {
  if (value == null || value === "") {
    return DEFAULT_API_URL;
  }
  if (typeof value !== "string") {
    throw new Error("actionbook: apiUrl must be a string");
  }

  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error(`actionbook: invalid apiUrl "${value}"`);
  }

  if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
    throw new Error(
      `actionbook: apiUrl must use http or https, got "${parsed.protocol}"`
    );
  }

  if (isPrivateHost(parsed.hostname)) {
    throw new Error(
      `actionbook: apiUrl must not point to private/local addresses, got "${parsed.hostname}"`
    );
  }

  return parsed.toString().replace(/\/$/, "");
}

function formatToolError(action: string, error: unknown) {
  if (isActionbookError(error)) {
    // Upstream SDK errors already have structured info — pass through markdown
    // errors and provide error code in details for agent observability
    const text = error.message.startsWith("## ")
      ? error.message
      : `## Error\n\nFailed to ${action}: ${error.message}`;
    return {
      content: [{ type: "text" as const, text }],
      details: { error: error.message, code: error.code },
    };
  }
  const message = error instanceof Error ? error.message : "Unknown error";
  return {
    content: [
      {
        type: "text" as const,
        text: `## Error\n\nFailed to ${action}: ${message}`,
      },
    ],
    details: { error: message },
  };
}

const actionbookPlugin = {
  id: "actionbook",
  name: "Actionbook",
  description:
    "Token-efficient browser automation with pre-verified selectors from Actionbook",

  register(api: OpenClawPluginApi) {
    let pluginConfig: ActionbookPluginConfig;
    try {
      const raw = api.pluginConfig ?? {};
      if (typeof raw !== "object" || raw === null) {
        throw new Error("actionbook: pluginConfig must be an object");
      }
      pluginConfig = raw as ActionbookPluginConfig;
    } catch (err) {
      api.logger.error(
        `Actionbook plugin config error: ${err instanceof Error ? err.message : String(err)}`
      );
      return;
    }

    const apiKey = pluginConfig.apiKey ?? "";
    let apiUrl: string;
    try {
      apiUrl = resolveApiUrl(pluginConfig.apiUrl);
    } catch (err) {
      api.logger.error(
        `Actionbook plugin URL error: ${err instanceof Error ? err.message : String(err)}`
      );
      return;
    }

    const client = new ApiClient(apiUrl, {
      apiKey,
      timeoutMs: 30000,
      retry: { maxRetries: 3, retryDelay: 1000 },
    });

    // ========================================================================
    // Lifecycle: inject Actionbook workflow guidance into agent context
    // ========================================================================

    api.on("before_prompt_build", async () => {
      return {
        prependSystemContext: ACTIONBOOK_GUIDANCE,
      };
    });

    // ========================================================================
    // Tool: search_actions
    // ========================================================================

    api.registerTool({
      name: "search_actions",
      label: "Actionbook Search",
      description: SEARCH_ACTIONS_DESCRIPTION,
      parameters: Type.Object({
        query: Type.String({
          minLength: 1,
          maxLength: 200,
          description:
            "Search keyword (e.g., 'airbnb search', 'login button')",
        }),
        domain: Type.Optional(
          Type.String({
            description: "Filter by domain (e.g., 'airbnb.com')",
          })
        ),
        background: Type.Optional(
          Type.String({
            description: "Context for search (improves relevance)",
          })
        ),
        url: Type.Optional(
          Type.String({ description: "Filter by specific page URL" })
        ),
        page: Type.Optional(
          Type.Integer({
            minimum: 1,
            description: "Page number (default: 1)",
          })
        ),
        page_size: Type.Optional(
          Type.Integer({
            minimum: 1,
            maximum: 100,
            description: "Results per page (1-100, default: 10)",
          })
        ),
      }),
      async execute(
        _toolCallId: string,
        params: {
          query: string;
          domain?: string;
          background?: string;
          url?: string;
          page?: number;
          page_size?: number;
        }
      ) {
        try {
          const result = await client.searchActions({
            query: params.query,
            domain: params.domain,
            background: params.background,
            url: params.url,
            page: params.page,
            page_size: params.page_size,
          });
          return {
            content: [{ type: "text" as const, text: result }],
            details: { query: params.query, domain: params.domain },
          };
        } catch (error) {
          return formatToolError("search actions", error);
        }
      },
    });

    // ========================================================================
    // Tool: get_action_by_area_id
    // ========================================================================

    api.registerTool({
      name: "get_action_by_area_id",
      label: "Actionbook Get Action",
      description: GET_ACTION_BY_AREA_ID_DESCRIPTION,
      parameters: Type.Object({
        area_id: Type.String({
          minLength: 1,
          description:
            "Area ID from search_actions (e.g., 'airbnb.com:/:default')",
        }),
      }),
      async execute(
        _toolCallId: string,
        params: { area_id: string }
      ) {
        try {
          const result = await client.getActionByAreaId(params.area_id);
          return {
            content: [{ type: "text" as const, text: result }],
            details: { area_id: params.area_id },
          };
        } catch (error) {
          return formatToolError("get action", error);
        }
      },
    });
  },
};

export default actionbookPlugin;
