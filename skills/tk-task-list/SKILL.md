---
name: tk-task-list
description: "Use when an agent needs persistent structured task tracking with the `tk` CLI: break complex work into tasks, coordinate multiple agents through a shared task list, claim the next available task, express dependencies, or keep progress across turns and sessions. Trigger this skill for multi-step implementation, migration, review, incident, or release work where `tk create`, `list`, `get`, `update`, `claim`, `next`, `block`, `done`, and `verify` should replace ad hoc TODO text."
---

# TK Task List

## Overview

Use `tk` as the durable source of truth for non-trivial work. Prefer explicit task state and dependencies over free-form notes, and prefer JSON output when another agent will consume the result.

## When to Use This Skill

- The work has 3 or more meaningful steps.
- The work spans multiple turns, sessions, or agents.
- The user asks for task tracking, TODO management, work decomposition, or "what's next".
- You need shared ownership and dependency tracking instead of a local in-memory checklist.

## When Not to Use This Skill

- The request is a single trivial action.
- You only need a temporary one-shot checklist in the current response.
- You need background process control or output capture. `tk` does not manage running commands.

## Quick Start

1. Confirm the CLI is available.
   - Prefer `tk --help`.
   - If `tk` is not installed but you are inside the `tk` source repo, use `cargo run -- <args>`.
2. Decide the list scope.
   - Solo repo work: the default repo-derived list is usually fine.
   - Shared work across agents, branches, releases, incidents, or migrations: set `--list <workstream>`.
3. Use JSON for agent-facing automation.
   - Prefer `tk --format json ...`.
4. If you need command details or option syntax, read [references/command-reference.md](references/command-reference.md).

## Working Rules

1. Create tasks with an imperative `subject` and a specific `description`.
2. Mark a task `in_progress` before doing the work when you are actively executing it.
3. Mark a task `completed` only when it is fully done.
4. Do not close tasks with unresolved blockers, failing tests, or partial implementation.
5. If work is blocked, encode the dependency with `tk block add` and create a separate unblocker task when needed.
6. Prefer lower numeric task IDs first unless a different order is clearly justified.
7. Use `tk get <id>` before `tk update <id>` if concurrent mutation is plausible.
8. Run `tk verify` after non-trivial dependency edits, `delete --detach`, or manual repair.

## Core Workflows

### Start a Fresh Task List

Use this when you are turning a vague or large task into explicit tracked work.

1. Initialize the list if needed.
   - `tk init`
   - Shared workstream: `tk --list release-2026-04 init`
2. Create the task set.
   - `tk create "Implement parser" --description "Add the parser entrypoint and wire it into the CLI."`
   - `tk create "Add regression tests" --description "Cover parser failures and happy paths."`
3. If ordering matters, add dependencies.
   - `tk block add 2 1`
4. Inspect the result.
   - `tk list`
   - `tk --format json list`

### Claim and Execute Work

Use this when one agent should take ownership of the next actionable task.

1. See what is available.
   - `tk next`
   - `tk --format json next`
2. Claim it explicitly.
   - `tk claim 3 --owner codex --start`
3. If you already know the owner identity should come from environment/config, you can omit `--owner`.
4. Update progress as work changes.
   - `tk update 3 --description "Expanded scope after schema audit"`
   - `tk done 3`

### Coordinate Multiple Agents

Use this when more than one agent or person shares a list.

1. Pick a shared list ID.
   - Example: `tk --list auth-migration ...`
2. Each agent should claim tasks explicitly.
   - `tk --list auth-migration claim 4 --owner reviewer --start`
3. Agents should use `tk next --claim --owner <name> --start` when pulling the next available item.
4. Keep blockers explicit so `next` does the right thing.
5. Prefer a stable owner name per agent, such as `codex`, `claude`, `reviewer`, or `ci`.

### Repair or Inspect Dependency State

Use this when task progression does not match expectations.

1. Read the relevant task.
   - `tk get 7`
2. Inspect blockers and claimability.
   - `tk --format json list --claimable`
3. Remove or add edges.
   - `tk block remove 7 3`
   - `tk block add 7 5`
4. Validate the whole list.
   - `tk verify`

## Suggested Command Patterns

```bash
tk init
tk create "Write spec" --description "Draft the initial specification"
tk create "Implement CLI" --description "Add the first working vertical slice"
tk block add 2 1
tk list
tk claim 1 --owner codex --start
tk done 1
tk next --claim --owner codex --start
tk verify
```

JSON-oriented variants:

```bash
tk --format json list
tk --format json get 1
tk --format json next --claim --owner codex --start
```

## Pitfalls

- Do not use `tk` as a process supervisor. It tracks tasks, not command execution.
- Do not assume `claim` implies `in_progress` unless `--start` is used.
- Do not assume completed tasks can move directly back to `in_progress`; reopen first.
- Do not parse text output in automation. Use JSON.
- Do not delete tasks with dependency edges unless you mean to detach them.

## Verification Checklist

- The intended list ID is correct.
- Task subjects are imperative and specific.
- Dependencies reflect real blocker order.
- Claimed tasks have the right owner.
- Completed tasks are actually done.
- `tk verify` is clean after structural changes.
