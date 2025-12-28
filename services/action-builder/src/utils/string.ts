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
