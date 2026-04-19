# tk

Standalone Rust CLI for persistent structured task lists.

## Install

Install via Homebrew:

```bash
brew tap mkusaka/tap
brew install mkusaka/tap/tk
```

Tagged releases publish Homebrew bottles for Apple Silicon and Intel Macs on
macOS Sequoia 15 and Tahoe 26. Until the first tagged release is published, or
on unsupported platforms, you can install from `HEAD`:

```bash
brew install --HEAD mkusaka/tap/tk
```

Install from a local checkout:

```bash
cargo install --path . --force
```

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

## Release

Pushing a `v*` tag runs the release workflow. It validates that the tag matches
`Cargo.toml`, creates a GitHub Release, builds Homebrew bottles for Apple
Silicon and Intel Macs on macOS 15 and 26, uploads them to the release, and
updates `mkusaka/homebrew-tap` directly. The workflow requires the
`HOMEBREW_TAP_TOKEN` repository secret.

```bash
git tag v0.0.1
git push origin v0.0.1
```
