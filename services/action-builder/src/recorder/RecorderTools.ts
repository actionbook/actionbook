import type OpenAI from "openai";

export function getRecorderTools(): OpenAI.Chat.Completions.ChatCompletionTool[] {
  return [
    {
      type: "function",
      function: {
        name: "navigate",
        description: "Navigate to a URL",
        parameters: {
          type: "object",
          properties: {
            url: { type: "string", description: "The URL to navigate to" },
          },
          required: ["url"],
        },
      },
    },
    {
      type: "function",
      function: {
        name: "observe_page",
        description:
          "Observe the current page to discover all interactive elements. Returns element details including selectors.",
        parameters: {
          type: "object",
          properties: {
            focus: {
              type: "string",
              description: "What to focus on when observing (e.g., 'search box and submit button')",
            },
            module: {
              type: "string",
              enum: ["header", "footer", "sidebar", "navibar", "main", "modal", "breadcrumb", "tab", "all"],
              description:
                "Page module to observe. Use 'all' to observe the entire page. If not specified, observes all modules.",
            },
          },
          required: ["focus"],
        },
      },
    },
    {
      type: "function",
      function: {
        name: "set_page_context",
        description: "Set the current page context (page type, name, and description)",
        parameters: {
          type: "object",
          properties: {
            page_type: {
              type: "string",
              description: "Page type identifier (e.g., 'home', 'search_results')",
            },
            page_name: { type: "string", description: "Human-readable page name" },
            page_description: { type: "string", description: "Optional page description" },
            url_pattern: { type: "string", description: "Optional URL pattern for this page type" },
          },
          required: ["page_type", "page_name"],
        },
      },
    },
    {
      type: "function",
      function: {
        name: "register_element",
        description:
          "Register a UI element capability with selectors, description, type, and allowed methods.",
        parameters: {
          type: "object",
          properties: {
            element_id: { type: "string", description: "Unique element identifier (snake_case)" },
            description: { type: "string", description: "Element description" },
            element_type: { type: "string", description: "Element type (button, input, link, etc)" },
            css_selector: { type: "string", description: "CSS selector for the element" },
            xpath_selector: { type: "string", description: "XPath selector for the element" },
            aria_label: { type: "string", description: "ARIA label for the element" },
            placeholder: { type: "string", description: "Placeholder text (for inputs)" },
            allow_methods: {
              type: "array",
              items: { type: "string" },
              description: "Allowed interaction methods (click, type, extract, etc)",
            },
            leads_to: { type: "string", description: "Optional page type this element leads to" },
            arguments: {
              type: "array",
              items: {
                type: "object",
                properties: {
                  name: { type: "string" },
                  type: { type: "string" },
                  description: { type: "string" },
                },
                required: ["name", "type"],
              },
              description: "Optional arguments for the element (e.g. input value)",
            },
            parent: { type: "string", description: "Optional parent element id" },
            depends_on: { type: "string", description: "Optional dependency element id" },
            visibility_condition: { type: "string", description: "Optional visibility condition description" },
            is_repeating: { type: "boolean", description: "Whether this element repeats in a list/table" },
            data_key: { type: "string", description: "Optional data extraction key" },
            children: {
              type: "array",
              items: { type: "string" },
              description: "Optional child element ids",
            },
            module: {
              type: "string",
              enum: ["header", "footer", "sidebar", "navibar", "main", "modal", "breadcrumb", "tab", "unknown"],
              description: "Page module where this element is located (e.g., header, main, sidebar)",
            },
            // Input-specific attributes
            input_type: {
              type: "string",
              description: "For input elements: the input type (text, email, password, number, search, tel, url, etc.)",
            },
            input_name: {
              type: "string",
              description: "For input elements: the name attribute",
            },
            input_value: {
              type: "string",
              description: "For input elements: the default/placeholder value",
            },
            // Link-specific attributes
            href: {
              type: "string",
              description: "For link elements: the href URL or pattern (e.g., '/search', 'https://example.com')",
            },
          },
          required: ["element_id", "description", "element_type", "allow_methods"],
        },
      },
    },
    {
      type: "function",
      function: {
        name: "interact",
        description:
          "Perform an interaction on a target element and register it (task-driven mode). Provide selectors when possible.",
        parameters: {
          type: "object",
          properties: {
            element_id: { type: "string", description: "Unique element identifier (snake_case)" },
            action: { type: "string", description: "Action to perform (click, type, etc)" },
            instruction: { type: "string", description: "Natural language instruction to locate the element" },
            value: { type: "string", description: "Optional value (for type action)" },
            element_description: { type: "string", description: "Element description" },
            css_selector: { type: "string", description: "Optional CSS selector for the element" },
            xpath_selector: { type: "string", description: "Optional XPath selector for the element" },
          },
          required: ["element_id", "action", "instruction", "element_description"],
        },
      },
    },
    {
      type: "function",
      function: {
        name: "wait",
        description: "Wait for some time or for text to appear",
        parameters: {
          type: "object",
          properties: {
            seconds: { type: "number", description: "Seconds to wait" },
            forText: { type: "string", description: "Text to wait for" },
          },
        },
      },
    },
    {
      type: "function",
      function: {
        name: "scroll",
        description: "Scroll the page up or down",
        parameters: {
          type: "object",
          properties: {
            direction: { type: "string", enum: ["up", "down"], description: "Scroll direction" },
            amount: { type: "number", description: "Scroll amount in pixels (default 300)" },
          },
          required: ["direction"],
        },
      },
    },
    {
      type: "function",
      function: {
        name: "scroll_to_bottom",
        description: "Scroll the page to the bottom to ensure lazy-loaded elements are loaded",
        parameters: {
          type: "object",
          properties: {
            wait_after_scroll: {
              type: "number",
              description: "Time to wait after scrolling in milliseconds (default 1000)",
            },
          },
          required: [],
        },
      },
    },
    {
      type: "function",
      function: {
        name: "go_back",
        description: "Navigate back to the previous page in browser history",
        parameters: {
          type: "object",
          properties: {},
          required: [],
        },
      },
    },
  ];
}

