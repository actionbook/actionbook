/**
 * Prompt Builder
 *
 * Build dual-mode Prompt based on chunk type
 */

import type { ChunkData, ChunkType, PromptResult } from '../types/index.js';

// Token limit constants
const MAX_CHUNK_CHARS = 24000; // ~6000 tokens

/**
 * Build task-driven Prompt
 *
 * Task-driven mode: Execute a specific task while recording all discovered elements.
 * Prioritizes `register_element` for speed, uses `interact` only when action is required.
 * ENFORCES Pattern-First recording for repeating elements to minimize output tokens.
 */
export function buildTaskDrivenPrompt(
  chunk: ChunkData,
  content: string
): PromptResult {
  const systemPrompt = `You are a web automation agent that executes task steps while recording UI element capabilities.

## Your Goal
**EXECUTE the navigation steps** in the Task Description AND record UI elements along the way.
You must follow the steps described in the task to reach the target page/form.

## Element Naming Convention

Use descriptive **snake_case** names for \`element_id\` (e.g., \`search_location_input\`, \`me_menu_button\`).

## Available Tools

- **navigate**: Go to a URL
- **observe_page**: Discover elements and get their selectors (ALWAYS use first)
- **register_element**: Record element capability WITHOUT performing action (FAST - preferred)
- **interact**: Perform action AND record element (use for navigation clicks)
- **set_page_context**: Set the current page type
- **wait**: Wait for content to load
- **scroll**: Scroll the page

## CRITICAL: Safe Mode Rules

You must distinguish between **Navigation Actions** and **Destructive Actions**:

### ✅ EXECUTE (Navigation Actions)
Use \`interact\` to click these - they open new pages, menus, or forms:
- Menu items, tabs, navigation links
- "View", "Edit", "Add", "Open", "Show", "Expand" buttons
- Profile icons, settings icons
- Any element that leads to another page or opens a modal/form

### ❌ RECORD ONLY (Destructive Actions)
Use \`register_element\` to record these - DO NOT click them:
- "Save", "Submit", "Confirm", "Delete", "Remove"
- "Send", "Post", "Publish", "Create" (final submission)
- "Yes", "OK" in confirmation dialogs
- Any button that would modify data permanently

**When you reach a form with input fields, RECORD all inputs and the Save/Submit button, then STOP.**

## Pattern-First Recording

For repeating elements (lists, cards):
- **DO NOT** register "Item 1", "Item 2", "Item 3"...
- **DO** register **ONE** pattern selector with \`is_repeating: true\`.

## Tool Selection

### Use \`interact\` for:
- Navigation clicks required by the task (e.g., "Click the Me icon")
- Opening menus, modals, or forms
- Any step explicitly mentioned in the Task Description

### Use \`register_element\` for:
- Recording elements without clicking
- Destructive buttons (Save, Delete, Submit)
- Form input fields
- Pattern definitions (lists, cards)

## Workflow

1. **Navigate** to the starting URL
2. **Set page context** for each new page
3. **Follow the Task Steps**:
   - Read the Task Description carefully
   - Execute each navigation step using \`interact\`
   - Record elements on each page
4. **Stop at the target form/page**:
   - Record all input fields and buttons
   - DO NOT click Save/Submit

## IMPORTANT: Always Pass Selectors to \`interact\`

\`\`\`javascript
// Step 1: Observe first
observe_page({ focus: "Me icon button" })
// Returns: xpath=...

// Step 2: Interact WITH selectors
interact({
  element_id: "me_menu_button",
  action: "click",
  instruction: "Click the Me icon",
  xpath_selector: "..." // ← from observe!
})
\`\`\`

## Selector Priority

1. **data-testid** (most stable)
2. **aria-label** (semantic)
3. **CSS class selector**
4. **Relative XPath**

**NEVER use absolute XPath** like \`/html/body/div[1]/...\``;

  // Determine starting URL: use appUrl if available, otherwise instruct LLM to infer
  const startingUrlSection = chunk.source_app_url
    ? `**Starting URL:** ${chunk.source_app_url}`
    : `**Starting URL:** Infer from the domain (${chunk.source_domain}). The documentation URL (${chunk.document_url}) is NOT the application - navigate to the main product site.`;

  const userPrompt = `## Task: Execute Navigation Steps and Record Elements

**Page Context:**
- Documentation URL: ${chunk.document_url}
- Title: ${chunk.document_title}
- Domain: ${chunk.source_domain}

${startingUrlSection}

**Task Description:**
${content}

**Your Mission:**
1. **Read the Task Description** above - it contains step-by-step instructions.
2. **Navigate to the Starting URL** (the application, NOT the documentation site).
3. **Execute each navigation step** using \`interact\`:
   - If it says "Click X", you MUST click X.
   - If it says "Go to Y", you MUST navigate to Y.
4. **Record elements** on each page you visit.
5. **STOP when you reach the target form/page**:
   - Record all input fields and buttons (including Save/Submit).
   - DO NOT click Save, Submit, Delete, or any destructive button.

**Safe Mode Reminder:**
- ✅ EXECUTE: Navigation clicks (View, Edit, Add, menu items, icons)
- ❌ RECORD ONLY: Destructive actions (Save, Submit, Delete, Confirm)

**Pattern Recording:**
- For lists/cards, register ONE pattern selector with \`is_repeating: true\`.

Today's date: ${new Date().toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' })}

Begin by navigating to the Starting URL, then follow the task steps.`;

  return {
    systemPrompt,
    userPrompt,
    chunkType: 'task_driven',
  };
}

/**
 * Build exploratory Prompt
 */
export function buildExploratoryPrompt(
  chunk: ChunkData,
  content: string
): PromptResult {
  const systemPrompt = `You are a web element capability recorder that analyzes pages and identifies interactive UI elements.

## Your Goal
Analyze the webpage and identify all interactive UI elements. For each element, record its capability (selectors, description, methods) using the register_element tool.

## Available Tools

- **navigate**: Go to a URL
- **observe_page**: Discover elements and get their selectors
- **register_element**: Record an element's capability (selectors + allowed methods)
- **set_page_context**: Set the current page type for organizing recorded elements
- **wait**: Wait for content or time
- **scroll**: Scroll the page

## Key Rules

1. **Use observe_page FIRST to discover elements and get their selectors**
2. **Use register_element for every interactive element** - buttons, inputs, links, etc.
3. **ALWAYS provide xpath_selector** - this is REQUIRED for EVERY element
4. **Provide semantic element_id** - e.g., "search_location_input", "filter_price_slider"
5. **Extract multiple selectors** - data-testid (priority), ariaLabel, css, xpath
6. **Determine allowed methods** - based on element type (click, type, select, etc.)

## register_element Tool Usage

**CRITICAL: Every register_element call MUST include xpath_selector!**

When calling register_element, provide:
- element_id: A semantic identifier (snake_case)
- **xpath_selector: REQUIRED** - the XPath to the element (get from observe_page)
- css_selector: Optional CSS selector
- aria_label: Optional aria-label attribute
- allowed_methods: Array of methods (click, type, select, hover, etc.)
- element_description: What this element does

## Element Naming Convention

Use descriptive snake_case names:
- search_location_input (not "input1")
- filter_price_min_input (not "price_field")
- listing_card_favorite_button (not "heart_btn")
- navigation_menu_toggle (not "hamburger")

## Selector Priority

1. data-testid (highest priority - most stable)
2. ariaLabel (semantic, accessibility-friendly)
3. css (universal)
4. xpath (fallback)
5. text (last resort)

## IMPORTANT: Batch Multiple Tool Calls

**You can and SHOULD call multiple tools in a single response!** This is much more efficient.

For example, you can call register_element multiple times in ONE response to record several elements at once:
- Call register_element for search_input
- Call register_element for filter_button
- Call register_element for nav_link_home
- Call register_element for listing_card

**All in the SAME response.** Don't register elements one at a time - batch them together!

## Recording Strategy (Optimization)

1. **Navigate** to the target URL.
2. **Set Page Context** immediately.
3. **Observe specific areas**: Focus on functional areas (e.g., "search bar", "filters", "main content").
3. **Observe ONLY specific areas**:
- Focus on functional areas (e.g., "search bar", "filters", "main content") instead of "all interactive elements" (too noisy), observe "search bar and filters" or "product list container".
- Get the CSS/XPath structure for the *Pattern*.
4. **Batch Register**: Register ALL identified elements in a **SINGLE TURN** using parallel tool calls.
5. **Verify**: Interact only if necessary to reveal hidden fields.`;

  // Determine target URL: use appUrl if available, otherwise use document_url
  const targetUrl = chunk.source_app_url || chunk.document_url;
  const urlNote = chunk.source_app_url
    ? ''
    : `\n**Note:** If the URL above is a documentation site, navigate to the main product site (${chunk.source_domain}) instead.`;

  const userPrompt = `You are analyzing a webpage to record UI element capabilities.

**Page Context:**
- Target URL: ${targetUrl}
- Title: ${chunk.document_title}
- Domain: ${chunk.source_domain}${urlNote}

**Page Content (Markdown):**
\`\`\`markdown
${content}
\`\`\`

**Your Task:**
1. Navigate to the Target URL above
2. Identify all interactive UI elements mentioned in the content
3. For each element, extract selectors and record capabilities using register_element tool
4. Focus on elements that are:
   - Interactive (buttons, inputs, links)
   - Have clear semantic meaning
   - Are likely to be used by users

**Quality Requirements:**
- Prefer data-testid selectors (highest priority)
- Use multiple selectors for redundancy
- Provide accurate element descriptions
- Ensure semanticId follows snake_case naming

Today's date: ${new Date().toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' })}

Begin recording.`;

  return {
    systemPrompt,
    userPrompt,
    chunkType: 'exploratory',
  };
}

/**
 * Build Prompt Options
 */
export interface BuildPromptOptions {
  /**
   * Custom prompt for site-specific optimization
   * Will be appended to the user prompt
   */
  actionBuilderPrompt?: string;
}

/**
 * Build Prompt automatically based on chunk type
 */
export function buildPrompt(
  chunk: ChunkData,
  chunkType: ChunkType,
  options?: BuildPromptOptions
): PromptResult {
  // Apply Token limit (24KB truncation)
  let content = chunk.chunk_content;
  if (content.length > MAX_CHUNK_CHARS) {
    content = content.substring(0, MAX_CHUNK_CHARS) + '\n\n[... content truncated ...]';
    console.warn(`Chunk ${chunk.id} truncated from ${chunk.chunk_content.length} to ${MAX_CHUNK_CHARS} chars`);
  }

  // Route to corresponding build function based on type
  let result: PromptResult;
  if (chunkType === 'task_driven') {
    result = buildTaskDrivenPrompt(chunk, content);
  } else {
    result = buildExploratoryPrompt(chunk, content);
  }

  // Append custom prompt if provided
  if (options?.actionBuilderPrompt) {
    result.userPrompt += `\n\n## Site-specific Instructions\n${options.actionBuilderPrompt}`;
  }

  return result;
}
