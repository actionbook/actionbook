/**
 * Truncate a string to a maximum length
 */
export function truncate(str: string, maxLength: number = 100): string {
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength - 3) + "...";
}

/**
 * Get visual length of a string (accounting for CJK characters)
 */
export function getVisualLength(str: string): number {
  // eslint-disable-next-line no-control-regex
  const nonAscii = str.match(/[^\x00-\xff]/g) || [];
  return str.length + nonAscii.length;
}

/**
 * Truncate a string based on visual length
 */
export function truncateVisual(str: string, maxLength: number): string {
  if (getVisualLength(str) <= maxLength) {
    return str;
  }

  const targetLen = maxLength - 3;
  let len = 0;
  let result = "";

  for (const char of str) {
    const charLen = getVisualLength(char);
    if (len + charLen > targetLen) {
      break;
    }
    result += char;
    len += charLen;
  }

  return result + "...";
}

/**
 * Pad end of string to target visual length
 */
export function padEndVisual(str: string, targetLength: number): string {
  const currentLen = getVisualLength(str);
  if (currentLen >= targetLength) return str;
  return str + " ".repeat(targetLength - currentLen);
}

/**
 * Format tool result for display
 */
export function formatToolResult(result: unknown): string {
  if (typeof result === "string") {
    return truncate(result, 500);
  }
  return truncate(JSON.stringify(result), 500);
}

/**
 * CSS special characters that need escaping in selectors
 * Reference: https://www.w3.org/TR/CSS21/syndata.html#characters
 * Note: Do NOT use /g flag with .test() - it causes lastIndex issues
 */
const CSS_SPECIAL_CHARS = /[!"#$%&'()*+,./:;<=>?@[\\\]^`{|}~]/;

/**
 * Check if an ID contains CSS special characters
 */
export function hasSpecialCssChars(id: string): boolean {
  return CSS_SPECIAL_CHARS.test(id);
}

/**
 * Create a safe CSS ID selector
 * If ID contains special characters (like `.`), uses attribute selector [id="..."]
 * Otherwise uses standard #id format
 *
 * @example
 * createIdSelector("simple") => "#simple"
 * createIdSelector("cs.AI") => '[id="cs.AI"]'
 * createIdSelector("my:id") => '[id="my:id"]'
 */
export function createIdSelector(id: string): string {
  if (!id) return "";

  // If ID contains special CSS characters, use attribute selector
  if (hasSpecialCssChars(id)) {
    // Escape quotes in the ID value
    const escapedId = id.replace(/"/g, '\\"');
    return `[id="${escapedId}"]`;
  }

  // Standard ID selector
  return `#${id}`;
}
