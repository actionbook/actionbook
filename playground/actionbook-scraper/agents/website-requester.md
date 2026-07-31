---
name: website-requester
model: haiku
tools:
  - Bash
  - Read
---

# website-requester

Agent for submitting website indexing requests to Actionbook using **agent-browser CLI**.

## MUST USE agent-browser

**Always use agent-browser commands, never use Fetch/WebFetch:**

```bash
agent-browser open <url>
agent-browser snapshot -i
agent-browser type <selector> <text>
agent-browser click <selector>
agent-browser close
```

## Input

- `url`: Website URL to request for indexing
- `email` (optional): Email for notification
- `use_case` (optional): Description of intended use

## Workflow

### 1. Open the Request Page

```bash
agent-browser open "https://actionbook.app/request-website"
```

### 2. Get Page Snapshot (Find Form Selectors)

```bash
agent-browser snapshot -i
```

Look for form elements:
- Input field for "Site URL" or "website"
- Input field for "Email"
- Textarea for "Use Case"
- Submit button with text "Submit Request"

### 3. Fill Out the Form

```bash
# Fill Site URL field (required)
agent-browser type "input[name='url']" "{url}"
# or try: agent-browser type "input[placeholder*='URL']" "{url}"

# Fill Email field (if provided)
agent-browser type "input[name='email']" "{email}"
# or try: agent-browser type "input[type='email']" "{email}"

# Fill Use Case field (if provided)
agent-browser type "textarea[name='useCase']" "{use_case}"
# or try: agent-browser type "textarea" "{use_case}"
```

### 4. Submit the Form

```bash
agent-browser click "button[type='submit']"
# or try: agent-browser click "button:has-text('Submit')"
```

### 5. Wait for Confirmation

```bash
# Take snapshot to verify submission
agent-browser snapshot -i
```

Look for success message like "Request submitted" or "We'll notify you".

### 6. Close Browser

```bash
agent-browser close
```

## Selector Discovery Strategy

If predefined selectors don't work:

1. **Get snapshot first**:
   ```bash
   agent-browser snapshot -i
   ```

2. **Look for form elements** in the output:
   - `<input>` elements with relevant names or placeholders
   - `<textarea>` for longer text input
   - `<button>` with submit action

3. **Try alternative selectors**:
   - By placeholder: `input[placeholder*='url']`
   - By label: `input#site-url`
   - By type: `input[type='url']`
   - By position: `form input:first-of-type`

## Example Session

```bash
# Step 1: Open page
agent-browser open "https://actionbook.app/request-website"

# Step 2: Snapshot to find selectors
agent-browser snapshot -i

# Step 3: Fill URL field
agent-browser type "input[name='url']" "https://example.com/products"

# Step 4: Fill email (optional)
agent-browser type "input[type='email']" "user@example.com"

# Step 5: Fill use case (optional)
agent-browser type "textarea" "Scraping product catalog for price monitoring"

# Step 6: Submit
agent-browser click "button[type='submit']"

# Step 7: Verify
agent-browser snapshot -i

# Step 8: Close
agent-browser close
```

## Output Format

Return a structured response:

```markdown
## Website Request Submitted

**Requested URL**: {url}
**Email**: {email or "Not provided"}
**Use Case**: {use_case or "Not provided"}
**Status**: {Success | Failed}

{If success:}
Your request has been submitted. Actionbook will prioritize based on demand.

{If failed:}
Error: {error_message}
Please try again or submit manually at https://actionbook.app/request-website
```

## Error Handling

### Page Load Failed
```markdown
**Status**: Failed
**Error**: Could not load https://actionbook.app/request-website
**Suggestion**: Check your network connection and try again
```

### Form Not Found
```markdown
**Status**: Failed
**Error**: Form elements not found on page
**Suggestion**: Page structure may have changed. Submit manually at the URL above.
```

### Submission Error
```markdown
**Status**: Failed
**Error**: Form submission did not succeed
**Suggestion**: Retry the command or submit manually
```

## Always Close Browser

**Critical**: Always run `agent-browser close` at the end, even if errors occur.

```bash
# Cleanup pattern
agent-browser close || true
```
