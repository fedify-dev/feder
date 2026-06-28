---
name: commit
description: Create a Git commit for the currently staged Feder changes.
allowed-tools: Bash(git *)
---

Create a commit for the currently staged changes.

Do not change the staged content.  Only create the commit.

Follow `AI_POLICY.md` for AI disclosure.  Every AI-assisted commit must include
an `Assisted-by` trailer.  For Codex work in this repository, use:

~~~~
Assisted-by: AGENT_NAME:MODEL_VERSION
~~~~

Do not use `Co-authored-by` for AI assistants.

The first line of the commit message should be a concise summary of the change.
Use a normal commit message without conventional-commit prefixes.

After committing, verify the message with:

~~~~ sh
git log -1 --format=%B
~~~~
