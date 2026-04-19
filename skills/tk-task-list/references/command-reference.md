# TK Command Reference

Use this file when you need option-level details while applying `$tk-task-list`.

## Core Commands

### Initialize and inspect

```bash
tk init
tk dir
tk list
tk get <id>
```

Useful flags:

- `--root <path>`: override storage root
- `--list <id>`: use a named task list
- `--format json`: machine-readable output

## Create and update

### Create

```bash
tk create "Subject"
tk create "Subject" --description "Detailed requirements"
tk create "Subject" --active-form "Implementing subject"
tk create "Subject" --visibility internal
tk create "Subject" --meta priority=high
```

Creation defaults:

- `status = pending`
- `owner = null`
- no dependencies

### Update

```bash
tk update <id> --subject "New subject"
tk update <id> --description "New description"
tk update <id> --status in_progress
tk update <id> --owner reviewer
tk update <id> --clear-owner
tk update <id> --set-meta priority=high
tk update <id> --unset-meta priority
```

Convenience aliases:

```bash
tk start <id>
tk done <id>
tk reopen <id>
```

## Ownership and task pulling

### Claim an explicit task

```bash
tk claim <id> --owner codex
tk claim <id> --owner codex --start
tk unclaim <id>
tk unclaim <id> --requeue
```

### Pull the next available task

```bash
tk next
tk next --claim --owner codex
tk next --claim --owner codex --start
```

`next` selects the lowest-ID claimable task.

Claimable means:

- `status == pending`
- owner is unset
- no unresolved blockers
- no missing blockers

## Dependencies

### Add blockers

```bash
tk block add <task-id> <blocker-id>...
```

Meaning:

- `<task-id>` cannot start until each `<blocker-id>` is completed.

### Remove blockers

```bash
tk block remove <task-id> <blocker-id>...
```

## Delete, reset, and verify

### Delete

```bash
tk delete <id>
tk delete <id> --detach
```

- plain `delete` is safe mode and fails if dependency edges exist
- `--detach` removes related edges first

### Reset

```bash
tk reset
tk reset --force
```

### Verify

```bash
tk verify
```

Checks include:

- schema validity
- asymmetric dependency edges
- missing blocker references
- dependency cycles

## Recommended Automation Patterns

### Structured planning

```bash
tk --format json list
tk --format json create "Implement parser" --description "..."
tk --format json update 1 --status in_progress
tk --format json done 1
```

### Shared multi-agent work

```bash
tk --list auth-migration --format json next --claim --owner codex --start
tk --list auth-migration --format json claim 4 --owner reviewer --start
tk --list auth-migration verify
```

## Notes

- Prefer JSON for agent-to-agent or agent-to-tool communication.
- Prefer text output only when directly showing humans a quick summary.
- Use `cargo run -- ...` instead of `tk ...` only when the binary is unavailable and you are already in the `tk` source repo.
