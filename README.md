![Actionbook Cover](https://github.com/user-attachments/assets/18f55ca3-2c25-4f6a-a518-1b07baf8b4dd)

<div align="center">

### Actionbook

![GitHub last commit](https://img.shields.io/github/last-commit/actionbook/actionbook) [![NPM Downloads](https://img.shields.io/npm/d18m/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![npm version](https://img.shields.io/npm/v/%40actionbookdev%2Fcli)](https://www.npmjs.com/package/@actionbookdev/cli) [![skills](https://img.shields.io/badge/skills-ready-blue)](https://skills.sh/actionbook/actionbook/actionbook)

**The Highest Accuracy Browser Automation Engine for AI Agents**
<br />
Actionbook provides up-to-date action manuals and DOM structure,
<br />
so your agent operates any website instantly without guessing.

[Website](https://actionbook.dev) · [GitHub](https://github.com/actionbook/actionbook) · [X](https://x.com/ActionbookHQ) · [Discord](https://actionbook.dev/discord)

</div>

<br />

## Why Actionbook?

### ❌ Without Actionbook

Building reliable browser agents is difficult and expensive:

- **Slow Execution:** Agents waste time parsing full HTML pages to find elements.
- **High Token Costs:** Sending entire DOM trees to LLMs consumes massive context windows.
- **Brittle Selectors:** Updates to website UIs break hardcoded selectors and agent logic immediately.
- **Hallucinations:** LLMs often guess incorrect actions when faced with complex, unstructured DOMs.

### ✅ With Actionbook

Actionbook places up-to-date action manuals with the relevant DOM selectors directly into your LLM's context.

- **10x Faster:** Agents access pre-computed "Action manuals" to know exactly what to do without exploring.
- **100x Token Savings:** Instead of whole HTML page, agents receive only related DOM elements in concise, semantic JSON definitions.
- **Resilient Automation:** Action manuals are maintained and versioned. If a site changes, the manual is updated, not your agent.
- **Universal Compatibility:** Works with any agent stack, including Claude Code, Codex, OpenClaw, and browser automation frameworks.

See how Actionbook enables an agent to complete an Airbnb search task 10x faster.

https://github.com/user-attachments/assets/9f896fe7-296a-44b3-8592-931a099612de

## Featured Skill: `active-research`

For X/Twitter research workflows, the most representative Actionbook skill is [`active-research`](./skills/active-research/SKILL.md).

It combines verified selectors, real browser automation, and structured report generation to research a live topic end to end.

### Demo: Analyze an X/Twitter Timeline

You can use Claude Code with Actionbook to operate the browser on X/Twitter, collect timeline activity and engagement signals, and turn the result into a structured summary.

https://github.com/user-attachments/assets/4705364e-02bc-48b2-956c-c806df746ad1

Example prompt:

```text
Use active-research to analyze today's X timeline activity around a topic and summarize the main themes.
```

By starting from Actionbook's verified selectors and action manuals instead of guessing the DOM, the agent can complete this workflow more accurately and reliably.

## Quick Start

Get started with Actionbook in under 2 minutes:

### 1. Install the CLI

### macOS / Linux

```bash
curl -fsSL https://actionbook.dev/install.sh | bash
```

### Windows

```powershell
irm https://actionbook.dev/install.ps1 | iex
```

### 2. Add one or more skills to your agent

```bash
npx skills add actionbook/actionbook
```

Skill support list:

- `actionbook`: the core browser-operation workflow. Best for browsing posts on X/Twitter and Reddit, collecting post and comment data, filling email flows and complex forms, and handling general page operations with less selector guesswork.
- `active-research`: a deep research workflow for multi-source browsing, structured collection, and long-form report generation.
- `extract`: a data extraction workflow that uses Actionbook selectors first, then generates a reusable Playwright script and structured output such as JSON or CSV.

## Example Use Cases

Actionbook playgrounds in this repo:

- [`arxiv-viewer`](./playground/arxiv-viewer): search, read, and analyze arXiv papers with a hybrid API + browser workflow
- [`lib-rs-scraper`](./playground/lib-rs-scraper): scrape lib.rs using Actionbook-verified selectors
- [`rust-learner`](./playground/rust-learner): query Rust language features and crate updates with browser-assisted workflows
- [`actionbook-scraper`](./playground/actionbook-scraper): generate reliable web scrapers with verified selectors and automatic validation
- [`article-exporter`](./playground/article-exporter): export web articles into clean structured content for downstream processing and publishing workflows
- [`deep-research`](./playground/deep-research): multi-source browsing, analysis, and report generation with Actionbook

More examples:

- [Examples Documentation](https://actionbook.dev/docs/examples)

## Integration Options

Use Actionbook in the way that fits your agent stack:

- **[CLI](https://actionbook.dev/docs/api-reference/cli)** for direct local usage.
- **[Skills](https://actionbook.dev/docs/guides/skills)** for better agent behavior and lower hallucination risk.
- **[OpenClaw Plugin](https://actionbook.dev/docs/openclaw)** for OpenClaw-based agent workflows with Actionbook.
- **[Dify Plugin](https://actionbook.dev/docs/guides/dify-plugin)** for using Actionbook selectors and browser automation inside Dify workflows.
- **[MCP Server](https://actionbook.dev/docs/guides/mcp-server)** for Cursor, Claude Code, VS Code, and similar clients.
- **[JavaScript SDK](https://actionbook.dev/docs/guides/sdk-integration)** for custom integrations.


## Follow the Build

We move fast. Star Actionbook on Github to support and get latest information.

[![Star Actionbook](https://github.com/user-attachments/assets/2d6571cb-4e12-438b-b7bf-9a4b68ef2be3)](https://github.com/actionbook/actionbook)

Join the community:

- [Chat with us on Discord](https://actionbook.dev/discord) - For help, feedback, workflow discussion, and sharing what you build with Actionbook
- [Follow @ActionbookHQ on X](https://x.com/ActionbookHQ) - For product updates, releases, and announcements

## Contributing

- **[Read the Contributing Guide](CONTRIBUTING.md)** - See repository setup, package layout, and validation workflows for the public repo.
- **[Request a Website](https://actionbook.dev/request-website)** - Suggest websites you want Actionbook to index.
- **[Join the Waitlist](https://actionbook.dev)** - We are currently in private beta. Join if you are interested in contributing or using Actionbook.

## License

See [LICENSE](LICENSE) for the license details.
