import type { BrowserAdapter } from "@actionbookdev/browser";
import { log } from "../utils/logger.js";
import { createIdSelector } from "../utils/string.js";
import type {
  ElementCapability,
  ElementType,
  AllowMethod,
  ArgumentDef,
  ObserveResultItem,
  SelectorItem,
  SelectorType,
  TemplateParam,
  PageModule,
} from "../types/index.js";

type ExtractMultipleSelectors = (
  observeResult: ObserveResultItem,
  cssSelector?: string,
  additionalAttributes?: {
    id?: string;
    dataTestId?: string;
    ariaLabel?: string;
    placeholder?: string;
    dataAttributes?: Record<string, string>;
  }
) => SelectorItem[];

type DetectTemplatePattern = (
  cssSelector: string
) => { template: string; params: TemplateParam[] } | null;

type InferElementType = (action: string, instruction: string) => ElementType;
type InferAllowMethods = (action: string) => AllowMethod[];

export type SetPageContext = (args: {
  pageType: string;
  pageName: string;
  pageDescription?: string;
  urlPattern?: string;
  concreteUrl?: string;
}) => void;

/** Return type for onNavigate handler */
export type NavigateResult = {
  isNew: boolean;
  reason?: 'already_visited' | 'external_domain';
};

export class RecorderToolExecutor {
  constructor(
    private browser: BrowserAdapter,
    private handlers: {
      ensureSiteCapability: (domain: string) => void;
      registerElement: (element: ElementCapability) => void;
      setPageContext: SetPageContext;
      /** Returns navigation result with isNew flag and optional reason */
      onNavigate: (url: string) => NavigateResult;
      /** Get current URL */
      getCurrentUrl: () => string | null;
      /** Get previous URL for going back */
      getPreviousUrl: () => string | null;
      extractMultipleSelectors: ExtractMultipleSelectors;
      detectTemplatePattern: DetectTemplatePattern;
      inferElementType: InferElementType;
      inferAllowMethods: InferAllowMethods;
    }
  ) {}

  async execute(
    toolName: string,
    toolArgs: Record<string, unknown>
  ): Promise<{ output: unknown; content: string }> {
    let output: unknown;

    switch (toolName) {
      case "navigate": {
        const url = toolArgs.url as string;
        const urlObj = new URL(url);

        // Check BEFORE navigation to avoid unnecessary page loads and side effects
        // This prevents: 1) Loading external/malicious sites, 2) Wasting time on heavy pages,
        // 3) Losing page state (forms, scroll position) when going back
        const navResult = this.handlers.onNavigate(url);

        // Normalize URL for display (same logic as ActionRecorder, includes hash for SPA support)
        const sortedParams = new URLSearchParams(urlObj.searchParams);
        sortedParams.sort();
        const urlKey = urlObj.pathname + (sortedParams.toString() ? '?' + sortedParams.toString() : '') + urlObj.hash;

        if (!navResult.isNew) {
          // URL should not be visited - reject without actual navigation
          if (navResult.reason === 'external_domain') {
            log("info", `[ActionRecorder] External domain rejected: ${urlObj.hostname}, navigation blocked`);
            output = {
              success: false,
              url: this.handlers.getCurrentUrl() || url,
              message: `Navigation to ${url} blocked: external domain (${urlObj.hostname}). Do not record elements from external domains. Please continue with elements on the current page.`,
              external_domain: true,
              blocked: true,
            };
          } else {
            // Already visited URL
            log("info", `[ActionRecorder] URL already visited: ${urlKey}, navigation blocked`);
            output = {
              success: false,
              url: this.handlers.getCurrentUrl() || url,
              message: `Navigation to ${urlKey} blocked: already visited. Please continue with other elements on the current page.`,
              already_visited: true,
              blocked: true,
            };
          }
        } else {
          // Valid new URL - perform actual navigation
          await this.browser.navigate(url);
          await this.browser.autoClosePopups();
          this.handlers.ensureSiteCapability(urlObj.hostname);
          output = { success: true, url };
        }
        break;
      }

      case "observe_page": {
        const focus = toolArgs.focus as string;
        const timeoutMs = toolArgs._timeoutMs as number | undefined;
        log("info", `[ActionRecorder] Observing page: ${focus}${timeoutMs ? ` (timeout: ${timeoutMs}ms)` : ''}`);

        const observeResults = await this.browser.observe(focus, timeoutMs);

        const elements: Array<{
          description: string;
          selector: string;
          method: string;
        }> = [];

        for (const item of observeResults) {
          if (item.description && item.selector) {
            elements.push({
              description: item.description,
              selector: item.selector,
              method: item.method || "click",
            });
          }
        }

        output = {
          elements_found: elements.length,
          elements: elements.slice(0, 20),
        };
        break;
      }

      case "interact": {
        const elementId = toolArgs.element_id as string;
        const action = toolArgs.action as string;
        const instruction = toolArgs.instruction as string;
        const value = toolArgs.value as string | undefined;
        const description = toolArgs.element_description as string;
        const providedXpath = toolArgs.xpath_selector as string | undefined;
        const providedCss = toolArgs.css_selector as string | undefined;

        log(
          "info",
          `[ActionRecorder] Interact: ${elementId} - ${action} - ${instruction}`
        );

        let selectors: SelectorItem[] = [];

        // Get xpath from LLM or observe_page
        let xpath: string | undefined;

        if (providedXpath || providedCss) {
          log("info", `[ActionRecorder] Using LLM-provided selectors for ${elementId}`);
          if (providedXpath) {
            xpath = providedXpath.replace(/^xpath=/, "");
          }
        } else {
          // Fall back to observe_page to get selectors
          log("info", `[ActionRecorder] No selectors provided, using observe_page for ${elementId}`);
          const observeResults = await this.browser.observe(instruction);

          if (observeResults.length > 0) {
            const observeResult = observeResults[0];
            if (observeResult.selector) {
              const sel = observeResult.selector;
              if (sel.startsWith("xpath=") || sel.startsWith("/")) {
                xpath = sel.replace(/^xpath=/, "");
              }
            }
          }
        }

        // Always try to extract additional selectors from xpath (id, dataTestId, ariaLabel, css)
        let additionalInfo: {
          id?: string;
          dataTestId?: string;
          ariaLabel?: string;
          placeholder?: string;
          cssSelector?: string;
          dataAttributes?: Record<string, string>;
        } | null = null;

        if (xpath) {
          additionalInfo = await this.browser.getElementAttributesFromXPath(xpath);
          log("info", `[ActionRecorder] Auto-extracted selectors for ${elementId}: id=${additionalInfo?.id}, dataTestId=${additionalInfo?.dataTestId}, css=${additionalInfo?.cssSelector}, aria=${additionalInfo?.ariaLabel}`);
        }

        // Build selectors from all available sources
        selectors = this.handlers.extractMultipleSelectors(
          { selector: xpath ? `xpath=${xpath}` : "", description: instruction },
          additionalInfo?.cssSelector,
          additionalInfo ? {
            id: additionalInfo.id,
            dataTestId: additionalInfo.dataTestId,
            ariaLabel: additionalInfo.ariaLabel,
            placeholder: additionalInfo.placeholder,
            dataAttributes: additionalInfo.dataAttributes,
          } : undefined
        );

        // If LLM also provided a CSS selector, add it if not already present
        if (providedCss) {
          const cssValue = providedCss.replace(/^css=/, "");
          const hasCSS = selectors.some(s => s.type === "css" && s.value === cssValue);
          if (!hasCSS) {
            selectors.unshift({
              type: "css" as SelectorType,
              value: cssValue,
              priority: 0,
              confidence: 0.75,
            });
          }
        }

        log("info", `[ActionRecorder] Extracted ${selectors.length} selectors for ${elementId}`);

        // Execute the action
        let actInstruction = instruction;
        if (action === "type" && value) {
          actInstruction = `${instruction} and type "${value}"`;
        }

        const actResult = await this.browser.act(actInstruction);

        // Register the element capability
        const elementCapability: ElementCapability = {
          id: elementId,
          selectors,
          description,
          element_type: this.handlers.inferElementType(action, instruction),
          allow_methods: this.handlers.inferAllowMethods(action),
          discovered_at: new Date().toISOString(),
        };

        if (value) {
          elementCapability.arguments = [
            {
              name: "value",
              type: "string",
              description: `Value to ${action}`,
            },
          ];
        }

        this.handlers.registerElement(elementCapability);

        output = {
          success: true,
          element_id: elementId,
          selectors,
          action_result: actResult,
        };
        break;
      }

      case "register_element": {
        // Validate element_id - reject undefined, null, empty, or "undefined" string
        const rawElementId = toolArgs.element_id as string | undefined;
        if (!rawElementId || rawElementId === "undefined" || rawElementId.trim() === "") {
          log("warn", `[RecorderToolExecutor] Skipping element with invalid element_id: ${rawElementId}`);
          output = { error: "invalid_element_id", message: "element_id is required and cannot be empty or undefined" };
          break;
        }

        let xpathSelector = toolArgs.xpath_selector as string | undefined;
        let cssSelector = toolArgs.css_selector as string | undefined;
        let ariaLabel = toolArgs.aria_label as string | undefined;
        let placeholder = toolArgs.placeholder as string | undefined;
        let elementId: string | undefined;
        let dataTestId: string | undefined;

        // Normalize XPath format (Stagehand may return "xpath=...")
        if (xpathSelector) {
          xpathSelector = xpathSelector.replace(/^xpath=/, "");
        }

        // ALWAYS try to find real selectors via observe_page when no valid XPath is provided
        // LLM often provides made-up CSS selectors that don't exist on the page
        const needsRealSelector = !xpathSelector;
        if (needsRealSelector) {
          try {
            const description = toolArgs.description as string;
            log("info", `[ActionRecorder] Finding real selector for ${toolArgs.element_id} via observe_page...`);
            const observeResult = await this.browser.observe(description);

            if (observeResult.length > 0) {
              const firstMatch = observeResult[0];
              const selector = firstMatch.selector;
              if (selector) {
                if (selector.startsWith("xpath=") || selector.startsWith("/")) {
                  xpathSelector = selector.replace(/^xpath=/, "");
                  log("info", `[ActionRecorder] Found XPath selector via observe: ${xpathSelector}`);
                } else {
                  // Only use observed CSS if we don't have one from LLM
                  const observedCss = selector.replace(/^css=/, "");
                  if (!cssSelector) {
                    cssSelector = observedCss;
                    log("info", `[ActionRecorder] Found CSS selector via observe: ${cssSelector}`);
                  }
                }
              }
            } else {
              log("warn", `[ActionRecorder] Could not find selector for ${toolArgs.element_id} via observe_page`);
            }
          } catch (err) {
            log("warn", `[ActionRecorder] Failed to auto-find selector for ${toolArgs.element_id}: ${err}`);
          }
        }

        // Try to extract additional selectors from XPath for complete selector coverage
        let optimizedXPath: string | undefined;
        if (xpathSelector) {
          const extractedAttrs = await this.browser.getElementAttributesFromXPath(xpathSelector);
          if (extractedAttrs) {
            // Prefer extracted CSS selector over LLM-provided one (which may be made up)
            if (extractedAttrs.cssSelector) {
              cssSelector = extractedAttrs.cssSelector;
            }
            // Always extract these stable selectors
            elementId = extractedAttrs.id;
            dataTestId = extractedAttrs.dataTestId;
            ariaLabel = ariaLabel || extractedAttrs.ariaLabel;
            // Prefer actual placeholder from page over LLM-provided value (LLM often guesses wrong)
            placeholder = extractedAttrs.placeholder || placeholder;
            // Use optimized XPath (attribute-based) instead of absolute path
            optimizedXPath = extractedAttrs.optimizedXPath;
            log("info", `[ActionRecorder] Auto-extracted selectors for ${toolArgs.element_id}: id=${elementId}, dataTestId=${dataTestId}, css=${cssSelector}, aria=${ariaLabel}, placeholder=${placeholder}, optimizedXPath=${optimizedXPath}`);
          }
        }

        // Build multi-selector format
        const multiSelectors: SelectorItem[] = [];
        let priority = 1;

        // 1. ID selector (highest priority)
        // Use createIdSelector to handle special characters like dots (e.g., "cs.AI" -> '[id="cs.AI"]')
        if (elementId) {
          multiSelectors.push({
            type: "id" as SelectorType,
            value: createIdSelector(elementId),
            priority: priority++,
            confidence: 0.95,
          });
        } else if (cssSelector?.startsWith("#") || cssSelector?.startsWith("[id=")) {
          multiSelectors.push({
            type: "id" as SelectorType,
            value: cssSelector,
            priority: priority++,
            confidence: 0.95,
          });
        }

        // 2. data-testid (very stable)
        if (dataTestId) {
          multiSelectors.push({
            type: "data-testid" as SelectorType,
            value: `[data-testid="${dataTestId}"]`,
            priority: priority++,
            confidence: 0.9,
          });
        }

        // 3. aria-label (stable)
        if (ariaLabel) {
          multiSelectors.push({
            type: "aria-label" as SelectorType,
            value: `[aria-label="${ariaLabel}"]`,
            priority: priority++,
            confidence: 0.85,
          });
        }

        // 4. CSS selector (check for template pattern)
        if (cssSelector && !cssSelector.startsWith("#")) {
          const templateInfo = this.handlers.detectTemplatePattern(cssSelector);
          if (templateInfo) {
            multiSelectors.push({
              type: "css" as SelectorType,
              value: templateInfo.template,
              priority: priority++,
              isTemplate: true,
              templateParams: templateInfo.params,
              confidence: 0.8,
            });
          } else {
            multiSelectors.push({
              type: "css" as SelectorType,
              value: cssSelector,
              priority: priority++,
              confidence: 0.75,
            });
          }
        }

        // 5. XPath (prefer optimized attribute-based XPath over absolute path)
        const finalXPath = optimizedXPath || xpathSelector;
        if (finalXPath) {
          multiSelectors.push({
            type: "xpath" as SelectorType,
            value: finalXPath,
            priority: priority++,
            // Higher confidence for attribute-based XPath, lower for absolute path
            confidence: optimizedXPath && optimizedXPath !== xpathSelector ? 0.8 : 0.6,
          });
        }

        // 6. Placeholder (for inputs)
        if (placeholder) {
          multiSelectors.push({
            type: "placeholder" as SelectorType,
            value: `[placeholder="${placeholder}"]`,
            priority: priority++,
            confidence: 0.65,
          });
        }

        // Skip elements with no valid selectors (phantom elements)
        // These are elements the LLM thinks exist but can't be found on the current page
        if (multiSelectors.length === 0) {
          log("warn", `[RecorderToolExecutor] Skipping element ${toolArgs.element_id} - no valid selectors found (element may not exist on current page)`);
          output = {
            error: "no_selectors_found",
            message: `Could not find element "${toolArgs.element_id}" on current page. The element may exist on a different page. Skip this element and continue with others.`,
            element_id: toolArgs.element_id,
          };
          break;
        }

        const elementCapability: ElementCapability = {
          id: toolArgs.element_id as string,
          selectors: multiSelectors,
          description: toolArgs.description as string,
          element_type: toolArgs.element_type as ElementType,
          allow_methods: toolArgs.allow_methods as AllowMethod[],
          leads_to: toolArgs.leads_to as string | undefined,
          arguments: toolArgs.arguments as ArgumentDef[] | undefined,
          discovered_at: new Date().toISOString(),
          // New fields for element relationships and data extraction
          parent: toolArgs.parent as string | undefined,
          depends_on: toolArgs.depends_on as string | undefined,
          visibility_condition: toolArgs.visibility_condition as string | undefined,
          is_repeating: toolArgs.is_repeating as boolean | undefined,
          data_key: toolArgs.data_key as string | undefined,
          children: toolArgs.children as string[] | undefined,
          // Page module classification
          module: toolArgs.module as PageModule | undefined,
          // Input-specific attributes
          input_type: toolArgs.input_type as string | undefined,
          input_name: toolArgs.input_name as string | undefined,
          input_value: (toolArgs.input_value as string) || undefined, // Skip empty string
          // Link-specific attributes
          href: toolArgs.href as string | undefined,
        };

        this.handlers.registerElement(elementCapability);
        output = { success: true, element_id: elementCapability.id };
        break;
      }

      case "set_page_context": {
        const pageType = toolArgs.page_type as string;
        const pageName = toolArgs.page_name as string;
        const pageDescription = toolArgs.page_description as string | undefined;
        const urlPattern = toolArgs.url_pattern as string | undefined;

        let currentUrl: string | undefined;
        try {
          currentUrl = this.browser.getUrl();
        } catch {
          // Ignore URL extraction errors
        }

        this.handlers.setPageContext({
          pageType,
          pageName,
          pageDescription,
          urlPattern,
          concreteUrl: currentUrl,
        });

        output = { success: true, page_type: pageType };
        break;
      }

      case "wait": {
        if (toolArgs.seconds) {
          const ms = (toolArgs.seconds as number) * 1000;
          await this.browser.wait(ms);
          output = { waited: toolArgs.seconds };
        } else if (toolArgs.forText) {
          await this.browser.waitForText(toolArgs.forText as string);
          output = { waitedFor: toolArgs.forText };
        } else {
          output = { waited: 0 };
        }
        break;
      }

      case "scroll": {
        const direction = toolArgs.direction as "up" | "down";
        const amount = (toolArgs.amount as number) || 300;
        await this.browser.scroll(direction, amount);
        output = { scrolled: direction, amount };
        break;
      }

      case "scroll_to_bottom": {
        const waitAfterScroll = (toolArgs.wait_after_scroll as number) || 1000;
        log("info", `[ActionRecorder] Scrolling to bottom (wait: ${waitAfterScroll}ms)`);
        await this.browser.scrollToBottom(waitAfterScroll);
        output = { success: true, wait_after_scroll: waitAfterScroll };
        break;
      }

      case "go_back": {
        log("info", `[ActionRecorder] Navigating back to previous page`);
        const previousUrl = this.handlers.getPreviousUrl?.();
        await this.browser.goBack();
        const currentUrl = this.handlers.getCurrentUrl?.();
        output = {
          success: true,
          previous_url: previousUrl,
          current_url: currentUrl,
        };
        break;
      }

      default:
        output = { error: `Unknown tool: ${toolName}` };
    }

    return { output, content: JSON.stringify(output) };
  }
}

