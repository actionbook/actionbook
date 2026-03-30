# Building CLIs for agents

If you've ever watched an agent try to use a CLI, you've seen it get stuck on an interactive prompt it can't answer, or parse a help page with no examples. Most CLIs were built assuming a human is at the keyboard. Here are some things I've found that make them work better for agents:

**Make it non-interactive.** If your CLI drops into a prompt mid-execution, an agent is stuck. It can't press arrow keys or type "y" at the right moment. Every input should be passable as a flag. Keep interactive mode as a fallback when flags are missing, not the primary path.

```bash
# this blocks an agent
$ mycli deploy
? Which environment? (use arrow keys)

# this works
$ mycli deploy --env staging
```

**Don't dump all your docs upfront.** An agent runs mycli, sees subcommands, picks one, runs mycli deploy --help, gets what it needs. No wasted context on commands it won't use. Let it discover things as it goes.

**Make --help actually useful.** Every subcommand gets a --help, and every --help includes examples. The examples do most of the work. An agent pattern-matches off mycli deploy --env staging --tag v1.2.3 faster than it reads a description.

```bash
$ mycli deploy --help
Options:
  --env     Target environment (staging, production)
  --tag     Image tag (default: latest)
  --force   Skip confirmation

Examples:
  mycli deploy --env staging
  mycli deploy --env production --tag v1.2.3
  mycli deploy --env staging --force
```

**Accept flags and stdin for everything.** Agents think in pipelines. They want to chain commands and pipe output between tools. Don't require positional args in weird orders or fall back to interactive prompts for missing values.

```bash
cat config.json | mycli config import --stdin
mycli deploy --env staging --tag $(mycli build --output tag-only)
```

**Fail fast with actionable errors.** If a required flag is missing, don't hang. Error immediately and show the correct invocation. Agents are good at self-correcting when you give them something to work with.

```bash
Error: No image tag specified.
  mycli deploy --env staging --tag <image-tag>
  Available tags: mycli build list --output tags
```

**Make commands idempotent.** Agents retry constantly. Network timeouts, context getting lost mid-task. Running the same deploy twice should return "already deployed, no-op", not create a duplicate.

**Add --dry-run for destructive actions.** Agents should be able to preview what a deploy or deletion would do before committing. Let them validate the plan, then run it for real.

```bash
$ mycli deploy --env production --tag v1.2.3 --dry-run
Would deploy v1.2.3 to production
  - Stop 3 running instances
  - Pull image registry.io/app:v1.2.3
  - Start 3 new instances
No changes made.

$ mycli deploy --env production --tag v1.2.3
✓ Deployed v1.2.3 to production
```

**--yes / --force to skip confirmations.** Humans get "are you sure?" and agents pass --yes to bypass it. Make the safe path the default but allow bypassing.

**Predictable command structure.** If an agent learns `mycli service list`, it should be able to guess `mycli deploy list` and `mycli config list`. Pick a pattern (resource + verb) and use it everywhere.

**Return data on success.** Show the deploy ID and the URL. Emojis are cute, but not really needed.

```bash
deployed v1.2.3 to staging
url: https://staging.myapp.com
deploy_id: dep_abc123
duration: 34s
```

If you're building CLIs that agents will use, these patterns save a lot of debugging time. Most of the work is just making explicit what humans figured out implicitly!
