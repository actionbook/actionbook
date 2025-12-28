/**
 * ActionId Utils - ActionId Encoding/Decoding Tool
 *
 * Handle ActionId encoding and decoding (with special character support)
 */

/**
 * Build ActionId
 *
 * Format: site/{domain}/page/{pageType}/element/{semanticId}
 * Special characters need URL encoding
 */
export function buildActionId(
  domain: string,
  pageType: string,
  semanticId: string
): string {
  return [
    'site',
    encodeURIComponent(domain),
    'page',
    encodeURIComponent(pageType),
    'element',
    encodeURIComponent(semanticId),
  ].join('/');
}

/**
 * Parse ActionId
 */
export function parseActionId(actionId: string): {
  domain: string;
  pageType: string;
  semanticId: string;
} {
  const parts = actionId.split('/');

  if (parts.length < 6 || parts[0] !== 'site' || parts[2] !== 'page' || parts[4] !== 'element') {
    throw new Error(`Invalid ActionId format: ${actionId}`);
  }

  return {
    domain: decodeURIComponent(parts[1] || ''),
    pageType: decodeURIComponent(parts[3] || ''),
    semanticId: decodeURIComponent(parts.slice(5).join('/') || ''),
  };
}
