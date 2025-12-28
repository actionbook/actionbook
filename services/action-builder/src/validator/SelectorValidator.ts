import type { Page } from "playwright";
import type { BrowserAdapter } from "../browser/BrowserAdapter.js";
import { log } from "../utils/logger.js";
import type {
  ValidatorConfig,
  ValidationResult,
  ElementValidationResult,
  SelectorValidationDetail,
  SiteCapability,
  ElementCapability,
  PageCapability,
  SelectorItem,
} from "../types/index.js";

type PreActionAction = "click" | "hover" | "type" | "wait" | "scroll";

type PreActionStep = {
  elementId?: string;
  action?: PreActionAction;
  value?: string;
};

type PreActionGroup = {
  pageType: string;
  elementIds: Set<string>;
  steps: PreActionStep[];
};

const DATE_PRE_ACTION_STEPS: PreActionStep[] = [
  { elementId: "search_location_input", action: "click" },
  { elementId: "search_location_input", action: "type", value: "Tokyo Shibuya Station" },
  { action: "wait", value: "600" },
  { elementId: "location_suggestion_item", action: "click" },
  { action: "wait", value: "600" },
  { elementId: "checkin_date_picker", action: "click" },
  { action: "wait", value: "1200" },
];

const FILTER_MODAL_STEPS: PreActionStep[] = [
  { elementId: "search_filters_button", action: "click" },
  { action: "wait", value: "1200" },
];

const FILTER_MODAL_SCROLL_STEPS: PreActionStep[] = [
  ...FILTER_MODAL_STEPS,
  { elementId: "property_type_section", action: "scroll" },
  { elementId: "show_results_button", action: "scroll" },
];

const PRE_ACTION_GROUPS: PreActionGroup[] = [
  {
    pageType: "home",
    elementIds: new Set([
      "calendar_date_13",
      "calendar_date_13_december",
      "calendar_date_15",
    ]),
    steps: DATE_PRE_ACTION_STEPS,
  },
  {
    pageType: "home",
    elementIds: new Set(["location_suggestion_item"]),
    steps: [
      { elementId: "search_location_input", action: "click" },
      { elementId: "search_location_input", action: "type", value: "Tokyo Shibuya Station" },
      { action: "wait", value: "600" },
    ],
  },
  {
    pageType: "search_results",
    elementIds: new Set([
      "price_range_filter",
      "room_bed_filter",
    ]),
    steps: FILTER_MODAL_STEPS,
  },
  {
    pageType: "search_results",
    elementIds: new Set([
      "property_type_section",
      "show_results_button",
    ]),
    steps: FILTER_MODAL_SCROLL_STEPS,
  },
];

/**
 * Selector Validator - Validates selector effectiveness
 */
export class SelectorValidator {
  private browser: BrowserAdapter;
  private config: ValidatorConfig;

  constructor(browser: BrowserAdapter, config: ValidatorConfig = {}) {
    this.browser = browser;
    this.config = {
      timeout: config.timeout || 5000,
      verbose: config.verbose || false,
      pageFilter: config.pageFilter,
      templateParams: config.templateParams,
    };
  }

  private async performWaitAction(value?: string): Promise<void> {
    const waitMs = value ? Number(value) : 500;
    await new Promise((resolve) => setTimeout(resolve, Number.isFinite(waitMs) ? waitMs : 500));
  }

  /**
   * Update config for dynamic validation options
   */
  updateConfig(config: Partial<ValidatorConfig>): void {
    this.config = { ...this.config, ...config };
  }

  /**
   * Validate all selectors for a site
   */
  async validate(capability: SiteCapability): Promise<ValidationResult> {
    const details: ElementValidationResult[] = [];
    let validCount = 0;
    let totalCount = 0;

    // Validate each page
    for (const [pageType, page] of Object.entries(capability.pages)) {
      // Apply page filter if specified
      if (this.config.pageFilter && this.config.pageFilter.length > 0) {
        if (!this.config.pageFilter.includes(pageType)) {
          log("info", `[SelectorValidator] Skipping page: ${pageType} (filtered)`);
          continue;
        }
      }

      const pageUrl = this.getPageUrl(capability.domain, page);

      if (!pageUrl) {
        log(
          "warn",
          `[SelectorValidator] No URL pattern for page: ${pageType}, skipping`
        );
        continue;
      }

      log("info", `[SelectorValidator] Validating page: ${pageType}`);

      try {
        await this.browser.navigate(pageUrl);
        await this.browser.autoClosePopups();

        let browserPage = await this.browser.getPage();

        for (const [elementId, element] of Object.entries(page.elements)) {
          totalCount++;

          const preActionGroup = this.getPreActionGroup(pageType, elementId);
          if (preActionGroup) {
            log(
              "info",
              `[SelectorValidator] Resetting page and running pre-actions for ${pageType}/${elementId}`
            );
            await this.browser.navigate(pageUrl);
            await this.browser.autoClosePopups();
            browserPage = await this.browser.getPage();
            log(
              "info",
              `[SelectorValidator] Page reset complete for ${pageType}/${elementId}, executing ${preActionGroup.steps.length} pre-action steps`
            );
            await this.runPreActions(
              browserPage,
              page,
              preActionGroup,
              this.config.templateParams
            );
            log(
              "info",
              `[SelectorValidator] Finished pre-actions for ${pageType}/${elementId}`
            );
          }

          const result = await this.validateElement(
            browserPage,
            element,
            pageType,
            this.config.templateParams
          );
          details.push(result);

          if (result.valid) {
            validCount++;
          }

          if (this.config.verbose) {
            const status = result.valid ? "✓" : "✗";
            // Show detailed selector results if available
            if (result.selectorsDetail && result.selectorsDetail.length > 0) {
              const selectorSummary = result.selectorsDetail
                .map(s => `${s.type}:${s.valid ? '✓' : '✗'}`)
                .join(', ');
              log("info", `  ${status} ${elementId} - [${selectorSummary}]`);
            } else {
              log(
                "info",
                `  ${status} ${elementId} - CSS: ${result.selector.css?.valid ? "valid" : "invalid"}, XPath: ${result.selector.xpath?.valid ? "valid" : "invalid"}`
              );
            }
          }
        }
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : String(error);
        log(
          "error",
          `[SelectorValidator] Failed to validate page ${pageType}: ${errorMessage}`
        );
      }
    }

    // Validate global elements
    if (Object.keys(capability.global_elements).length > 0) {
      log("info", "[SelectorValidator] Validating global elements");

      const mainUrl = `https://${capability.domain}`;
      try {
        await this.browser.navigate(mainUrl);
        await this.browser.autoClosePopups();

        const browserPage = await this.browser.getPage();

        for (const [elementId, element] of Object.entries(
          capability.global_elements
        )) {
          totalCount++;
          const result = await this.validateElement(
            browserPage,
            element,
            "global"
          );
          details.push(result);

          if (result.valid) {
            validCount++;
          }

          if (this.config.verbose) {
            const status = result.valid ? "✓" : "✗";
            log("info", `  ${status} ${elementId}`);
          }
        }
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : String(error);
        log(
          "error",
          `[SelectorValidator] Failed to validate global elements: ${errorMessage}`
        );
      }
    }

    const validationRate = totalCount > 0 ? validCount / totalCount : 0;

    return {
      success: validationRate >= 0.8,
      domain: capability.domain,
      totalElements: totalCount,
      validElements: validCount,
      invalidElements: totalCount - validCount,
      validationRate,
      details,
    };
  }

  /**
   * Validate a single element's selectors
   */
  private async validateElement(
    page: Page,
    element: ElementCapability,
    pageType: string,
    templateParams?: Record<string, string>
  ): Promise<ElementValidationResult> {
    const result: ElementValidationResult = {
      elementId: element.id,
      pageType,
      valid: false,
      selector: {},
      selectorsDetail: [],
    };

    // Validate selectors array
    for (const sel of element.selectors) {
      const detail = await this.validateSingleSelector(page, sel, templateParams);
      result.selectorsDetail!.push(detail);

      // Update legacy format for backward compatibility
      if (detail.valid) {
        if (sel.type === 'css' || sel.type === 'id' || sel.type === 'data-testid' ||
            sel.type === 'aria-label' || sel.type === 'placeholder') {
          result.selector.css = { valid: true };
        } else if (sel.type === 'xpath') {
          result.selector.xpath = { valid: true };
        }
      }
    }

    // Element is valid if any selector works
    result.valid = result.selectorsDetail!.some(d => d.valid);

    return result;
  }

  /**
   * Validate a single SelectorItem
   */
  private async validateSingleSelector(
    page: Page,
    selector: SelectorItem,
    templateParams?: Record<string, string>
  ): Promise<SelectorValidationDetail> {
    const resolved = this.resolveSelectorValue(selector, templateParams);
    if (resolved.error) {
      return {
        type: selector.type,
        value: selector.value,
        valid: false,
        error: resolved.error,
        isTemplate: selector.isTemplate,
      };
    }

    const selectorValue = resolved.value;

    try {
      const locator = this.buildLocator(page, selector.type, selectorValue);
      const count = await locator.count();
      return {
        type: selector.type,
        value: selectorValue,
        valid: count > 0,
        error: count === 0 ? 'Element not found' : undefined,
        isTemplate: selector.isTemplate,
      };
    } catch (error) {
      return {
        type: selector.type,
        value: selectorValue,
        valid: false,
        error: error instanceof Error ? error.message : String(error),
        isTemplate: selector.isTemplate,
      };
    }
  }

  private buildLocator(page: Page, selectorType: SelectorItem['type'], selectorValue: string) {
    switch (selectorType) {
      case 'xpath':
        return page.locator(`xpath=${selectorValue}`).first();
      case 'text':
        return page.locator(`text=${selectorValue}`).first();
      default:
        return page.locator(selectorValue).first();
    }
  }

  private resolveSelectorValue(
    selector: SelectorItem,
    templateParams?: Record<string, string>
  ): { value: string; error?: string } {
    let selectorValue = selector.value;

    if (selector.isTemplate && selector.templateParams) {
      if (!templateParams) {
        return {
          value: selector.value,
          error: 'Template selector requires parameters',
        };
      }

      for (const param of selector.templateParams) {
        const placeholder = `{{${param.name}}}`;
        const value = templateParams[param.name];
        if (value) {
          selectorValue = selectorValue.replace(placeholder, value);
        } else {
          return {
            value: selector.value,
            error: `Missing template parameter: ${param.name}`,
          };
        }
      }
    }

    return { value: selectorValue };
  }

  private getPreActionGroup(pageType: string, elementId: string): PreActionGroup | undefined {
    return PRE_ACTION_GROUPS.find(
      (group) => group.pageType === pageType && group.elementIds.has(elementId)
    );
  }

  private async runPreActions(
    page: Page,
    pageCapability: PageCapability,
    group: PreActionGroup,
    templateParams?: Record<string, string>
  ): Promise<void> {
    for (const [index, step] of group.steps.entries()) {
      const action = step.action ?? 'click';

      if (action === 'wait') {
        log(
          "info",
          `[SelectorValidator] Pre-action ${index + 1}/${group.steps.length} (${group.pageType}): waiting ${step.value ?? "500"}ms`
        );
        await this.performWaitAction(step.value);
        continue;
      }

      if (!step.elementId) {
        log(
          "warn",
          `[SelectorValidator] Pre-action step missing elementId for action ${action} on page ${group.pageType}`
        );
        continue;
      }

      const targetElement = pageCapability.elements[step.elementId];
      if (!targetElement || !targetElement.selectors || targetElement.selectors.length === 0) {
        log(
          "warn",
          `[SelectorValidator] Pre-action element ${step.elementId} not found or missing selectors on page ${group.pageType}`
        );
        continue;
      }

      log(
        "info",
        `[SelectorValidator] Pre-action ${index + 1}/${group.steps.length} (${group.pageType}): ${action} on ${step.elementId}`
      );

      await this.performActionForElement(
        page,
        targetElement,
        action,
        step.value,
        templateParams
      );

      if (typeof (page as any).waitForTimeout === 'function') {
        await (page as any).waitForTimeout(300);
      }
    }
  }

  private async performActionForElement(
    page: Page,
    element: ElementCapability,
    action: PreActionAction,
    value?: string,
    templateParams?: Record<string, string>
  ): Promise<void> {
    for (const selector of element.selectors ?? []) {
      const resolved = this.resolveSelectorValue(selector, templateParams);
      if (resolved.error) {
        continue;
      }

      try {
        const locator = this.buildLocator(page, selector.type, resolved.value);
        if (action === 'hover') {
          await locator.hover({ timeout: this.config.timeout });
        } else if (action === 'type') {
          await locator.fill(value ?? '', { timeout: this.config.timeout });
        } else if (action === 'scroll') {
          if (typeof (locator as any).scrollIntoViewIfNeeded === 'function') {
            await (locator as any).scrollIntoViewIfNeeded({ timeout: this.config.timeout });
          } else {
            await locator.evaluate((el) => el.scrollIntoView({ behavior: 'smooth', block: 'center' }));
          }
        } else {
          await locator.click({ timeout: this.config.timeout });
        }
        log(
          "info",
          `[SelectorValidator] Pre-action success for ${element.id} using ${selector.type}`
        );
        return;
      } catch (error) {
        log(
          'warn',
          `[SelectorValidator] Pre-action failed for ${element.id} using ${selector.type}: ${error instanceof Error ? error.message : String(error)}`
        );
      }
    }

    throw new Error(`[SelectorValidator] Failed to perform pre-action on ${element.id}`);
  }

  /**
   * Get a URL to test for a page
   */
  private getPageUrl(domain: string, page: PageCapability): string | null {
    // First check for concrete_url (saved during recording)
    if ((page as any).concrete_url) {
      return (page as any).concrete_url;
    }

    if (page.url_patterns.length > 0) {
      // Try to use the first URL pattern as a concrete URL
      const pattern = page.url_patterns[0];

      // Check if it's a regex pattern (contains .*, \., etc.)
      const isRegexPattern = /\.\*|\\\.|\[.*?\]|\(.*?\)/.test(pattern);

      if (pattern.startsWith("http") && !isRegexPattern) {
        // It's a concrete URL, use it directly
        return pattern;
      }

      // If it's a regex pattern, try to convert to a usable URL
      if (isRegexPattern) {
        // For home page pattern like https://www\.airbnb\.com/?$
        if (page.page_type === "home" || pattern.endsWith("/?$") || pattern.endsWith("/$")) {
          // Convert regex escapes to normal URL
          const url = pattern
            .replace(/\\\./g, ".")  // \. -> .
            .replace(/\??\$$/, "") // Remove ?$ or $ at end
            .replace(/\/\?$/, "/"); // /? -> /
          return url;
        }

        // For other patterns with .*, we can't use them
        log("warn", `[SelectorValidator] URL pattern "${pattern}" requires navigation, skipping page: ${page.page_type}`);
        return null;
      }
    }

    // Default to main domain for common page types
    switch (page.page_type) {
      case "home":
        return `https://${domain}/`;
      default:
        // For other page types, we need a concrete URL
        log("warn", `[SelectorValidator] No concrete URL for page: ${page.page_type}, skipping`);
        return null;
    }
  }
}
