# @actionbookdev/tools-ai-sdk

Vercel AI SDK-compatible tools for [Actionbook](https://actionbook.app) - enabling AI agents to access accurate, real-time website operation information (element selectors, operation methods, page structure).

The package provides two main tools:

- **searchActions** - Search for action manuals by keyword (e.g., "airbnb search", "google login")
- **getActionById** - Get complete action details including DOM selectors and step-by-step instructions

## Installation

```bash
npm install @actionbookdev/tools-ai-sdk ai zod
# or
pnpm add @actionbookdev/tools-ai-sdk ai zod
```

## Usage

```typescript
import { searchActions, getActionById } from '@actionbookdev/tools-ai-sdk'
import { generateText } from 'ai'
import { openai } from '@ai-sdk/openai'

const { text } = await generateText({
  model: openai('gpt-4o'),
  prompt: 'Find the login button selector for Airbnb',
  tools: {
    searchActions: searchActions(),
    getActionById: getActionById(),
  },
})

console.log(text)
```

## Typical Workflow

1. **Search for actions**: Use `searchActions` to find relevant actions
2. **Get action details**: Use `getActionById` with the returned action ID
3. **Use selectors**: Extract CSS/XPath selectors from the response
4. **Automate**: Use selectors with Playwright or other browser automation tools

```typescript
const result = await generateText({
  model: openai('gpt-5'),
  prompt: `
    Find the search input selector for Airbnb and write Playwright code to:
    1. Navigate to Airbnb
    2. Type "Tokyo" in the search input
    3. Click the search button
  `,
  tools: {
    searchActions: searchActions(),
    getActionById: getActionById(),
  },
})
```

## Configuration (Optional)

By default, Actionbook API works without authentication. For higher rate limits, you can set an API key:

```bash
export ACTIONBOOK_API_KEY=your-api-key
```

Or pass options directly:

```typescript
import { searchActions } from '@actionbookdev/tools-ai-sdk'

const tool = searchActions({
  apiKey: 'your-api-key',
  timeoutMs: 60000,
})
```

## License

Apache-2.0
