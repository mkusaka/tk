# tk Task List CLI Specification

Status: Draft

Last updated: 2026-04-19

## 1. Purpose

`tk` is a standalone Rust CLI for structured task-list management.

It extracts the persistent task-list portion of Claude Code's task tools, removes Anthropic-specific assumptions, and defines a vendor-neutral interface that can be used by:

- Codex CLI
- Claude Code
- custom agents
- CI jobs
- human operators

The CLI is designed to be subprocess-friendly first. Any agent should be able to use it without linking an SDK by executing `tk ... --format json` and reading stdout/stderr plus exit codes.

## 2. Goals

- Provide a single-binary Rust CLI for persistent task tracking.
- Preserve the useful semantics of the original structured task list:
  - create tasks
  - update status
  - assign owners
  - express dependency edges
  - claim the next available task
- Support concurrent access from multiple agents and terminals.
- Offer stable machine-readable JSON output.
- Remain vendor-neutral:
  - no Anthropic terminology
  - no Claude-specific prompts or reminders
  - no mailbox, teammate, or model-specific behavior
- Default to project-scoped storage, while allowing explicit shared roots.

## 3. Non-goals

`tk` does not cover the background execution subsystem. The following are explicitly out of scope for v1:

- shell/background process management
- task output capture/log files for running commands
- remote session tracking
- sub-agent transcript viewing
- UI overlays, spinners, or REPL-specific panels
- automatic reminder injection into model prompts
- vendor-specific hooks such as "task created" or "task completed" callbacks

If those features are needed later, they should be built as separate tools on top of `tk`, not inside the core task-list CLI.

## 4. Design Principles

- Local-first: state is stored as plain files on disk.
- Agent-safe: JSON mode is the contract; text mode is for humans.
- Explicit over implicit: no hidden auto-cleanup, no hidden prompt mutation.
- Concurrency-safe: file locks and atomic writes are required.
- Portable: Linux and macOS are first-class; Windows is best-effort.
- Inspectable: users can open task JSON files directly when debugging.

## 5. Core Concepts

| Term | Meaning |
| --- | --- |
| Root | Filesystem directory under which `tk` stores all state |
| List | A named task list, usually corresponding to one project or workstream |
| Task | A single work item in a list |
| Owner | Free-form agent or human identifier responsible for a task |
| Blocker | Another task that must complete before a task is claimable |
| Claimable | `pending`, unowned, and with no unresolved blockers |
| Revision | Monotonic version number used for optimistic concurrency |

## 6. Configuration Resolution

### 6.1 Root path

`tk` resolves the storage root in this order:

1. `--root <path>`
2. `TK_ROOT`
3. nearest VCS root (`.git` or `.jj`) plus `/.tk`
4. current working directory plus `/.tk`

This makes project-local storage the default while still allowing an explicit shared location for cross-worktree or cross-machine setups.

### 6.2 List ID

`tk` resolves the active list ID in this order:

1. `--list <id>`
2. `TK_LIST_ID`
3. `<root>/config.toml` `default_list_id`
4. sanitized basename of the detected VCS root
5. literal `default`

### 6.3 Default owner

`tk` resolves the default owner in this order:

1. `--owner <name>` on commands that support it
2. `TK_OWNER`
3. `<root>/config.toml` `default_owner`
4. unset

### 6.4 Config file

Optional config file path:

`<root>/config.toml`

Initial v1 keys:

```toml
default_list_id = "repo-name"
default_owner = "codex"
output_format = "json"
```

CLI flags always override config.

## 7. Storage Layout

```text
<root>/
  config.toml
  lists/
    <list-id>/
      manifest.json
      .lock
      .highwatermark
      tasks/
        1.json
        2.json
        3.json
```

### 7.1 Path rules

- `list-id` must match: `[a-z0-9][a-z0-9._-]{0,127}`
- task file names are decimal task IDs plus `.json`
- root and list directories should be created with user-private permissions when possible

### 7.2 Manifest file

`manifest.json` stores list-level metadata:

```json
{
  "schema_version": 1,
  "list_id": "repo-name",
  "title": "repo-name",
  "description": null,
  "created_at": "2026-04-19T12:34:56Z",
  "updated_at": "2026-04-19T12:34:56Z",
  "list_revision": 0
}
```

`list_revision` increments on every mutation that changes the list:

- create
- update
- claim
- unclaim
- delete
- block add/remove
- reset

`.highwatermark` stores the maximum numeric ID ever assigned so deleted IDs are never reused.

## 8. Persistent Task Schema

Each task file stores exactly one JSON object.

```json
{
  "schema_version": 1,
  "id": "12",
  "revision": 3,
  "subject": "Run integration tests",
  "description": "Run the full integration suite after parser changes.",
  "active_form": "Running integration tests",
  "status": "in_progress",
  "visibility": "public",
  "owner": "codex",
  "blocks": ["14"],
  "blocked_by": ["9", "10"],
  "metadata": {
    "component": "parser",
    "priority": "high"
  },
  "created_at": "2026-04-19T12:34:56Z",
  "updated_at": "2026-04-19T12:40:00Z",
  "started_at": "2026-04-19T12:35:10Z",
  "completed_at": null
}
```

### 8.1 Field semantics

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Decimal string, unique within the list |
| `revision` | integer | Starts at 1, increments on every task mutation |
| `subject` | string | Short actionable title |
| `description` | string | Full task description |
| `active_form` | string or null | Optional present-continuous phrasing for UI clients |
| `status` | enum | `pending`, `in_progress`, `completed` |
| `visibility` | enum | `public` or `internal` |
| `owner` | string or null | Responsible human/agent identifier |
| `blocks` | string[] | Downstream task IDs blocked by this task |
| `blocked_by` | string[] | Upstream blocker task IDs |
| `metadata` | object | Arbitrary JSON object |
| `created_at` | RFC3339 UTC string | Creation timestamp |
| `updated_at` | RFC3339 UTC string | Last mutation timestamp |
| `started_at` | RFC3339 UTC string or null | First transition to `in_progress` |
| `completed_at` | RFC3339 UTC string or null | Transition to `completed` |

### 8.2 Validation limits

- `subject`: 1 to 200 UTF-8 characters
- `description`: 0 to 32768 UTF-8 bytes
- `active_form`: 0 to 120 UTF-8 characters
- `owner`: 1 to 128 UTF-8 characters when set
- `metadata`: JSON object, serialized size up to 65536 bytes

## 9. Derived Fields

JSON responses for `list`, `get`, `claim`, `next`, and `watch` may include derived fields that are not persisted directly:

| Field | Meaning |
| --- | --- |
| `open_blocked_by` | subset of `blocked_by` whose referenced tasks are not completed |
| `invalid_blocked_by` | blocker IDs that do not resolve to an existing task |
| `claimable` | true when `status == pending`, `owner == null`, `open_blocked_by` is empty, and `invalid_blocked_by` is empty |
| `blocked_tasks` | convenience alias for `blocks` in human-readable output only |

Derived fields must never be written back into task JSON files.

## 10. State Model

### 10.1 Canonical statuses

- `pending`
- `in_progress`
- `completed`

### 10.2 Allowed transitions

Default allowed transitions:

- `pending -> in_progress`
- `in_progress -> pending`
- `in_progress -> completed`

Restricted transitions:

- `pending -> completed` requires `--force`
- `completed -> pending` requires explicit `tk reopen <id>` or `tk update <id> --status pending --force`
- `completed -> in_progress` requires explicit reopen first

### 10.3 Ownership behavior

- Ownership and status are orthogonal.
- Claiming a task does not automatically mark it `in_progress` unless `--start` is specified.
- Unclaiming a task does not automatically revert status unless `--requeue` is specified.

## 11. Dependency Model

### 11.1 Invariants

- Dependency edges form a directed acyclic graph.
- Self-dependencies are invalid.
- `blocks` and `blocked_by` are symmetric views of the same edge set.
- Mutations that add edges must update both sides transactionally.

### 11.2 Blocker semantics

A task is considered blocked when any referenced blocker task:

- exists and is not `completed`, or
- does not exist

Missing blocker references are treated as invalid graph state. They make a task non-claimable and must be surfaced by `verify`.

### 11.3 Delete semantics

`tk delete <id>` defaults to safe mode:

- fail if the task participates in any dependency edge

`tk delete <id> --detach`:

- remove all inbound/outbound edges transactionally
- delete the task
- bump the revisions of affected tasks

## 12. CLI Surface

### 12.1 Global flags

All commands accept:

- `--root <path>`
- `--list <id>`
- `--format <text|json|ndjson>`
- `--no-color`
- `--quiet`

`ndjson` is only valid for streaming commands such as `watch`.

### 12.2 Command summary

| Command | Purpose |
| --- | --- |
| `tk init` | initialize root and list manifest |
| `tk dir` | print resolved root/list/task paths |
| `tk create` | create a task |
| `tk list` | list tasks |
| `tk get <id>` | fetch one task |
| `tk update <id>` | patch a task |
| `tk start <id>` | convenience alias for `update --status in_progress` |
| `tk done <id>` | convenience alias for `update --status completed` |
| `tk reopen <id>` | move completed task back to `pending` |
| `tk claim <id>` | assign owner, optionally start |
| `tk unclaim <id>` | clear owner, optionally requeue |
| `tk next` | find or claim the next available task |
| `tk block add <task-id> <blocker-id>...` | add blockers |
| `tk block remove <task-id> <blocker-id>...` | remove blockers |
| `tk delete <id>` | delete a task |
| `tk reset` | delete all tasks from a list |
| `tk verify` | validate graph and on-disk state |
| `tk watch` | stream list changes |

## 13. Command Details

### 13.1 `tk init`

Behavior:

- create `<root>` if missing
- create list directory and manifest if missing
- no-op if already initialized
- optional flags:
  - `--title <text>`
  - `--description <text>`

### 13.2 `tk dir`

Behavior:

- print resolved root path
- print resolved list path
- in JSON mode also include:
  - `manifest_path`
  - `tasks_dir`
  - `lock_path`
  - `highwatermark_path`

### 13.3 `tk create`

Required:

- `subject`

Optional:

- `--description <text>`
- `--active-form <text>`
- `--owner <name>`
- `--visibility <public|internal>`
- `--meta key=value` repeated
- `--json-body <file>` for large descriptions or metadata

Behavior:

- allocate the next decimal ID under list-level lock
- write task file atomically
- initialize:
  - `status = pending`
  - `revision = 1`
  - empty dependency arrays

### 13.4 `tk list`

Optional filters:

- `--status <pending|in_progress|completed>` repeated
- `--owner <name>`
- `--unowned`
- `--claimable`
- `--all` to include `visibility=internal`
- `--limit <n>`

Sorting:

- default: ascending numeric ID
- optional future extension: `--sort updated_at`

### 13.5 `tk get <id>`

Behavior:

- return full persisted task plus derived fields
- fail with `task_not_found` if missing

### 13.6 `tk update <id>`

Supported mutations:

- `--subject <text>`
- `--description <text>`
- `--active-form <text>`
- `--status <pending|in_progress|completed>`
- `--owner <name>`
- `--clear-owner`
- `--visibility <public|internal>`
- `--set-meta key=value` repeated
- `--unset-meta <key>` repeated
- `--if-revision <n>`
- `--force`

Rules:

- patch only the specified fields
- increment `revision` only if anything changed
- set `started_at` the first time status becomes `in_progress`
- set `completed_at` when status becomes `completed`
- clear `completed_at` when reopening
- `--if-revision` performs compare-and-swap and returns `revision_conflict` on mismatch

### 13.7 `tk claim <id>`

Required:

- `--owner <name>` unless resolved from config/env

Optional:

- `--start`
- `--check-busy`
- `--if-revision <n>`

Behavior:

- reject if task is missing
- reject if task is already owned by a different owner
- reject if task is not claimable
- `--check-busy` rejects when the same owner already holds another unresolved task
- `--start` also sets status to `in_progress`

### 13.8 `tk unclaim <id>`

Optional:

- `--requeue` to set status back to `pending`
- `--if-revision <n>`

Behavior:

- clear owner
- if `--requeue`, also set `status = pending`

### 13.9 `tk next`

Behavior:

- select the lowest-ID claimable task
- if `--claim --owner <name>` is given, claim it atomically
- if `--start` is combined with `--claim`, also mark it `in_progress`

If no task is available:

- JSON mode returns `ok: false` with `code = no_available_task`
- process exits with code 3

### 13.10 `tk block add`

Form:

`tk block add <task-id> <blocker-id>...`

Meaning:

- `<task-id>` is blocked by each `<blocker-id>`

Behavior:

- validate all referenced tasks exist
- reject self-edge
- reject cycle introduction
- update both `blocked_by` and `blocks` transactionally

### 13.11 `tk block remove`

Form:

`tk block remove <task-id> <blocker-id>...`

Behavior:

- remove the edge from both sides
- succeed even if the edge is already absent

### 13.12 `tk delete <id>`

Optional:

- `--detach`
- `--if-revision <n>`

Behavior:

- safe delete by default
- `--detach` removes related dependency edges first
- updates `.highwatermark` if needed

### 13.13 `tk reset`

Optional:

- `--force`

Behavior:

- delete all task files in the active list
- preserve `.highwatermark`
- bump `list_revision`
- without `--force`, fail if any task is not `completed`

### 13.14 `tk verify`

Checks:

- manifest readability
- task schema validity
- duplicate IDs
- asymmetric dependency edges
- missing blocker references
- dependency cycles
- invalid timestamps

JSON output includes a list of diagnostics with stable codes.

### 13.15 `tk watch`

Behavior:

- start by emitting a full snapshot
- continue emitting change events
- use filesystem watch with polling fallback
- best-effort only; no durable replay contract in v1

Event types:

- `snapshot`
- `task_created`
- `task_updated`
- `task_deleted`
- `list_reset`

`watch` must use `--format ndjson`.

## 14. Output Contract

### 14.1 JSON success envelope

```json
{
  "ok": true,
  "command": "create",
  "list": {
    "list_id": "repo-name",
    "list_revision": 4
  },
  "task": {
    "id": "12",
    "revision": 1,
    "subject": "Run integration tests"
  }
}
```

### 14.2 JSON error envelope

```json
{
  "ok": false,
  "command": "claim",
  "error": {
    "code": "blocked",
    "message": "Task #12 is blocked by unresolved tasks",
    "details": {
      "task_id": "12",
      "open_blocked_by": ["9", "10"]
    }
  }
}
```

### 14.3 Stability guarantees

- JSON field names are the stable machine contract.
- Text output is human-oriented and may change between minor releases.
- NDJSON event shapes are stable once v1 is released.

## 15. Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | success |
| `1` | usage error or unexpected internal error |
| `2` | not found |
| `3` | conflict, blocked, busy, or no available task |
| `4` | validation error |
| `5` | storage error or lock timeout |
| `130` | interrupted by signal |

## 16. Concurrency and Atomicity

- `create`, `next --claim`, `reset`, and graph-wide delete operations must use list-level locking.
- ordinary single-task `update` may use task-level locking.
- `claim --check-busy` must use list-level locking because it depends on scanning unresolved tasks.
- all writes must use:
  1. read current state
  2. validate
  3. write temp file
  4. fsync temp file when supported
  5. atomic rename
- commands that mutate multiple task files must either complete fully or fail without partial graph corruption.

## 17. Agent Integration Contract

The primary intended use pattern is:

1. `tk list --format json` or `tk next --format json`
2. if claiming work, `tk claim` or `tk next --claim --start --owner <agent>`
3. read full context with `tk get <id> --format json`
4. update state explicitly with `tk update`
5. mark completion explicitly with `tk done` or `tk update --status completed`

Rules for integrators:

- Do not parse human text output.
- Use JSON mode and exit codes only.
- Do not assume hidden reminders or auto-state transitions.
- Treat `revision_conflict` as a normal retry condition.
- Use owner names that are stable within a project, such as `codex`, `claude`, `ci`, or `alice`.

## 18. Migration from Claude Code Task Tools

### 18.1 Mapping

| Claude Code concept | `tk` equivalent |
| --- | --- |
| `TaskCreateTool` | `tk create` |
| `TaskListTool` | `tk list --format json` |
| `TaskGetTool` | `tk get <id> --format json` |
| `TaskUpdateTool` | `tk update <id> ...` |
| `status: deleted` | `tk delete <id>` |
| `metadata._internal` | `visibility = internal` |

### 18.2 Intentional differences

`tk` intentionally removes the following Claude Code behaviors:

- no automatic expansion of a TUI task pane
- no hidden prompt reminders to use task tools
- no auto-reset 5 seconds after all tasks complete
- no mailbox notifications on owner assignment
- no Anthropic-specific team terminology

## 19. Rust Implementation Guidance

This document defines CLI behavior, not crate layout, but the recommended implementation split is:

- `tk-core`
  - schemas
  - storage
  - locking
  - graph validation
- `tk-cli`
  - `clap` command parsing
  - text formatting
  - JSON envelope rendering
  - watch loop

Recommended crates:

- `clap`
- `serde`
- `serde_json`
- `toml`
- `camino`
- `fs4` or equivalent locking crate
- `notify` for watch support
- `thiserror`
- `time`

Only the CLI JSON/NDJSON interface is a compatibility surface in v1. Internal Rust APIs may change.

## 20. Future Extensions

Potential later additions, explicitly out of scope for this spec:

- task priorities
- labels
- archival instead of delete/reset
- task comments
- durable event log with replay cursors
- SQLite backend
- HTTP daemon mode
