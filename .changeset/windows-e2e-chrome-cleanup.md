---
"@actionbookdev/cli": patch
---

Fix Windows Chrome process cleanup and improve orphan recovery.

- Replace NtQueryInformationProcess/ToolHelp with Win32 Job Objects for atomic termination of Chrome helper processes (renderer, GPU, utility)
- Delete Chrome singleton lock files before orphan kill so new sessions can start even if helper processes linger
- Add actionable error message when orphan Chrome still holds the user-data-dir lock
- Fix 54 Windows e2e test cross-platform compatibility issues
