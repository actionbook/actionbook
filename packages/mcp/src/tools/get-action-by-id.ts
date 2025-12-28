import { z } from 'zod'
import { defineTool } from './index.js'
import { ApiClient } from '../lib/api-client.js'
import { formatActionDetail } from '../lib/formatter.js'

export const GetActionByIdInputSchema = z.object({
  id: z
    .number()
    .int()
    .positive('Action ID must be a positive integer')
    .describe('Action ID (numeric chunk ID, e.g., 123, 456)'),
})

export type GetActionByIdInput = z.infer<typeof GetActionByIdInputSchema>

export function createGetActionByIdTool(
  apiClient: Pick<ApiClient, 'getActionById'>
) {
  return defineTool({
    name: 'get_action_by_id',
    description: `Get complete action details by numeric action ID, including content and UI element selectors.

**What you get:**
- Full action content/documentation
- Page element selectors (CSS, XPath)
- Element types and allowed methods (click, type, extract, etc.)
- Document metadata (title, URL, creation time)

**UI Elements Information:**
When available, you'll get structured element information including:
- CSS selectors and XPath selectors for precise element location
- Element types (button, list_item, data_field, etc.)
- Allowed methods for each element (click, type, extract, etc.)
- Element dependencies and relationships

**Use returned selectors with browser automation:**
\`\`\`javascript
// Example: Using CSS selector from Actionbook
const selector = '.company-list li';
await page.locator(selector).click();

// Example: Using XPath selector
const xpath = '//div[@class="company-list"]//li';
await page.locator(xpath).first().click();
\`\`\`

**Typical workflow:**
1. Search for actions: search_actions({ query: "company card" })
2. Get action_id from results (e.g., 123)
3. Get full details: get_action_by_id({ id: 123 })
4. Extract selectors from UI Elements section
5. Use selectors in your browser automation script

**Selector Priority:**
Prefer CSS selectors when available - they are most commonly supported and reliable.
Use XPath selectors as fallback for complex DOM traversal.`,
    inputSchema: GetActionByIdInputSchema,
    handler: async (input: GetActionByIdInput): Promise<string> => {
      const detail = await apiClient.getActionById(input.id)
      return formatActionDetail(detail)
    },
  })
}
