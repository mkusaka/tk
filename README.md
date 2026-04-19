# tk

Standalone Rust CLI for persistent structured task lists.

## Status

Early implementation based on [spec.md](./spec.md) and [spec.ja.md](./spec.ja.md).

## Commands

```bash
cargo install --path .
cargo run -- init
cargo run -- create "Implement parser"
cargo run -- list --format json
cargo run -- claim 1 --owner codex --start
cargo run -- done 1
```

## Scope

`tk` manages structured task lists only.

Out of scope:

- background shell execution
- task output capture
- remote agent sessions
- REPL-specific UI
