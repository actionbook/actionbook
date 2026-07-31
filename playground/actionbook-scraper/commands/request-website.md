# /actionbook-scraper:request-website

Request a new website to be indexed in Actionbook by submitting its URL through the request form.

## Usage

```
/actionbook-scraper:request-website <url> [--email <email>] [--use-case <description>]
```

## Parameters

- `url` (required): The URL of the website you want indexed in Actionbook
- `--email` (optional): Your email to receive notification when the site is available
- `--use-case` (optional): Brief description of your use case (helps prioritize)

## Examples

```
/actionbook-scraper:request-website https://example.com/products
/actionbook-scraper:request-website https://newsite.com/data --email user@example.com
/actionbook-scraper:request-website https://company.com/api --use-case "scraping product catalog for price comparison"
```

## When to Use

Use this command when:
1. `/actionbook-scraper:analyze` returns "No matching actions found"
2. `/actionbook-scraper:list-sources` doesn't include your target site
3. You want to request priority indexing for a specific website

## Workflow

1. **Launch website-requester agent** (uses agent-browser)
2. Agent opens `https://actionbook.app/request-website`
3. Agent fills out the request form:
   - Site URL: the provided URL
   - Your Email: provided email (if any)
   - Use Case: provided description (if any)
4. Agent clicks "Submit Request" button
5. Returns confirmation of submission

## Agent

This command uses the **website-requester** agent (haiku model) with agent-browser CLI.

## Output Format

```markdown
## Website Request Submitted

**Requested URL**: {url}
**Status**: Submitted successfully

Actionbook will prioritize indexing based on user demand.
You'll be notified at {email} when selectors are available.

### Next Steps
1. Check back later with `/actionbook-scraper:list-sources`
2. Try `/actionbook-scraper:analyze {url}` once the site is indexed
```

## Error Handling

| Error | Cause | Solution |
|-------|-------|----------|
| "Form not found" | Page structure changed | Report issue to Actionbook |
| "Submission failed" | Network or form error | Retry the command |
| "Invalid URL format" | Malformed URL provided | Provide full URL with https:// |

## Notes

- Requests are prioritized based on user demand
- Popular sites are indexed faster
- You can submit multiple requests for different pages on the same domain
- The email notification is optional but recommended to know when selectors are ready
