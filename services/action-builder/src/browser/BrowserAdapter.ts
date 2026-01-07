import type { Page } from "playwright";
import type { ActionObject, ObserveResultItem } from "../types/index.js";

export interface BrowserAdapter {
  initialize(): Promise<void>;
  getPage(): Promise<Page>;
  navigate(url: string): Promise<void>;

  observe(instruction: string, timeoutMs?: number): Promise<ObserveResultItem[]>;
  act(instructionOrAction: string | ActionObject): Promise<unknown>;
  actWithSelector(action: ActionObject): Promise<unknown>;

  autoClosePopups(): Promise<number>;

  getElementAttributesFromXPath(
    xpathSelector: string
  ): Promise<{
    id?: string;
    dataTestId?: string;
    ariaLabel?: string;
    placeholder?: string;
    cssSelector?: string;
    tagName?: string;
    dataAttributes?: Record<string, string>;
    name?: string;
    textContent?: string;
    optimizedXPath?: string;
  } | null>;

  wait(ms: number): Promise<void>;
  waitForText(text: string, timeout?: number): Promise<void>;
  scroll(direction: "up" | "down", amount?: number): Promise<void>;

  /** Scroll to the bottom of the page to load lazy-loaded content */
  scrollToBottom(waitAfterMs?: number): Promise<void>;

  /** Navigate back to the previous page in browser history */
  goBack(): Promise<void>;

  /** Get accumulated token usage statistics (optional - implemented by StagehandBrowser) */
  getTokenStats?(): { input: number; output: number; total: number };

  close(): Promise<void>;
}

