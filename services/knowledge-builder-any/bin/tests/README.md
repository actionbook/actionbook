# Test Utility Tools

This directory contains utility binaries for testing and verifying the knowledge-builder-any service.

## Available Tools

### 1. `create_test_task`

Creates a simple test build_task for testing the worker.

**Usage**:
```bash
cargo run --bin create_test_task
```

**Output**: Creates a task for `https://example.com`

---

### 2. `create_unique_test_task`

Creates a test task with a unique URL (using timestamp) to avoid conflicts.

**Usage**:
```bash
cargo run --bin create_unique_test_task
```

**Output**: Creates a task for `https://example-{timestamp}.com`

---

### 3. `create_real_test_task`

Creates a test task for a real, accessible website (httpbin.org).

**Usage**:
```bash
cargo run --bin create_real_test_task
```

**Output**: Creates a task for `https://httpbin.org`

**Note**: Checks if the source already exists before creating.

---

### 4. `verify_url_format`

Verifies that documents have the correct URL format with fragment identifiers.

**Usage**:
```bash
# Auto-detect latest source
cargo run --bin verify_url_format

# Verify specific source
cargo run --bin verify_url_format 1771
```

**What it checks**:
- ✅ URL format (should use `#handbook-action` and `#handbook-overview`)
- ✅ No `.md` suffix in URLs
- ✅ No url_hash conflicts
- ✅ Chunks distribution

**Example output**:
```
Found 2 documents:

ID         URL                                                Title
----------------------------------------------------------------------------------------------------
10048      https://httpbin.org#handbook-action                Action Handbook
10049      https://httpbin.org#handbook-overview              Overview

✅ All documents use fragment format correctly!
✅ Fix is working as expected!
✅ No url_hash conflicts detected!
```

---

### 5. `cleanup_test_data`

🗑️ **Clean up test data** from database to avoid pollution.

**Usage**:
```bash
# Clean up specific source
cargo run --bin cleanup_test_data -- 1771

# Clean up latest created source
cargo run --bin cleanup_test_data -- --latest

# Clean up all test sources (name contains 'test')
cargo run --bin cleanup_test_data -- --all-test
```

**What it deletes** (with confirmation):
- ✅ Chunks associated with documents
- ✅ Documents
- ✅ Recording tasks and steps
- ✅ Build tasks
- ✅ Source versions
- ✅ The source itself

**Safety features**:
- Shows detailed info before deletion
- Requires explicit "yes" confirmation
- Deletes in correct dependency order

**Example output**:
```
📋 Sources to be deleted:

  Source ID: 1771
  Name: HTTPBin Test Service
  URL: https://httpbin.org
  Created: 2026-01-05 00:52:17
  └─ Documents: 2
     └─ Chunks: 12
  └─ Build Tasks: 1
  └─ Versions: 1

⚠️  This will permanently delete the above data!
Continue? (yes/no): yes

🗑️  Cleaning up source 1771...
  ✓ Deleted 12 chunks
  ✓ Deleted 2 documents
  ✓ Deleted 1 versions
  ✓ Deleted 1 build tasks
  ✓ Deleted source
✓ Source 1771 cleaned up successfully

✅ Cleanup complete!
```

---

## Typical Workflow

### Testing URL Fix

1. **Create** a test task:
   ```bash
   cargo run --bin create_real_test_task
   ```

2. **Run** the worker to process it:
   ```bash
   cargo run --release -- worker --once
   ```

3. **Verify** the results:
   ```bash
   cargo run --bin verify_url_format
   ```

4. **Clean up** test data:
   ```bash
   cargo run --bin cleanup_test_data -- --latest
   ```

---

## Important: Always Clean Up Test Data! 🧹

**Why?**
- Avoids database pollution
- Keeps test environment clean
- Prevents confusion with production data

**When to clean up:**
- ✅ After each test run
- ✅ Before committing code
- ✅ When switching test scenarios

**Quick cleanup:**
```bash
# After testing, run this immediately
cargo run --bin cleanup_test_data -- --latest
```

---

## Requirements

- PostgreSQL database running with correct schema
- `.env` file with `DATABASE_URL` configured
- For embedding tests: `OPENAI_API_KEY` configured

---

## Related Documentation

- **Test Plan**: `../../.docs/test-url-fix-verification.md`
- **Test Results**: `../../.docs/test-result-summary.md`
- **Unit Tests**: `../../tests/test_url_hash_conflict.rs`
- **SQL Verification**: `../../.docs/verify-url-fix.sql`
