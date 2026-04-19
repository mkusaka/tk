---
name: tk-release
description: "Use when releasing this `tk` repository to GitHub and publishing Homebrew bottles to `mkusaka/homebrew-tap`. Trigger this skill when the user asks to cut a versioned release, prepare the next tag, publish Homebrew, verify release automation, or confirm that `vX.Y.Z` is shipped end-to-end. Use it for version alignment, preflight checks, CI/watch steps, tag push, release verification, and tap update verification."
---

# TK Release

## Overview

Release this repository by aligning `Cargo.toml` with the target version, pushing `main`, waiting for `CI`, creating a `v*` tag, and verifying that the release workflow publishes bottles and updates `mkusaka/homebrew-tap`.

## Preconditions

- Work from the `tk` repo root.
- The working tree is clean unless the user explicitly wants to include new changes.
- `HOMEBREW_TAP_TOKEN` is configured in the GitHub repository secrets.
- The remote `mkusaka/homebrew-tap` already contains `Formula/tk.rb` and the generic `update-formula.yml` receiver.

If any precondition is not true, stop and fix it before tagging.

## Core Workflow

### 1. Pick and align the target version

1. Read the current package version.
   - `sed -n 's/^version = "\\(.*\\)"$/\\1/p' Cargo.toml | head -n1`
2. If the requested release version differs, update it first.
3. The git tag must be exactly `v<package-version>`.
4. Do not push a tag that disagrees with `Cargo.toml`; the release workflow rejects it.

### 2. Run the local preflight

Run these checks before pushing:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --locked --all-targets
cargo test
cargo package --allow-dirty --locked --no-verify
```

Use `--allow-dirty` only for a local packaging smoke check before commit. The workflow itself runs from a clean checkout.

### 3. Push `main` and wait for CI

1. Commit the release-preparation changes.
2. Push `main`.
3. Watch the `CI` workflow until it is green.

Useful commands:

```bash
git push origin main
gh run list --limit 10 --json databaseId,workflowName,status,conclusion,headSha
gh run watch <run-id> --exit-status
gh run view <run-id> --log-failed
```

If `CI` fails, fix the issue, commit, push again, and re-watch until green.

### 4. Create and push the release tag

Once `main` is green:

```bash
git tag v0.0.1
git push origin v0.0.1
```

Replace `0.0.1` with the actual target version. Never hard-code the example version without checking `Cargo.toml`.

### 5. Watch the release workflow

After the tag push:

1. Find the `Release` workflow run for the tag.
2. Watch it through:
   - `verify`
   - `release`
   - `bottle`
   - `publish`
   - `update_homebrew_tap`

Useful commands:

```bash
gh run list --workflow Release --limit 10
gh run watch <run-id> --exit-status
gh run view <run-id> --log-failed
```

## What “done” means

Do not report success until all of the following are true:

1. The `Release` workflow concluded successfully.
2. The GitHub release for `vX.Y.Z` exists and is not draft.
3. Bottle artifacts were uploaded to the release.
4. `mkusaka/homebrew-tap` `main` contains `Formula/tk.rb` updated to the released version.

Useful verification commands:

```bash
gh release view v0.0.1
gh release view v0.0.1 --json isDraft,tagName,url
gh api repos/mkusaka/homebrew-tap/contents/Formula/tk.rb --jq '.download_url'
curl -L "$(gh api repos/mkusaka/homebrew-tap/contents/Formula/tk.rb --jq '.download_url')"
```

## Failure Handling

### Tag/version mismatch

Symptom:

- release workflow fails in `Validate tag version`

Fix:

- update `Cargo.toml` to the intended version or recreate the tag to match

### Missing `HOMEBREW_TAP_TOKEN`

Symptom:

- release workflow fails in `Validate release configuration`

Fix:

- add the secret in the `mkusaka/tk` GitHub repository settings

### Homebrew tap update does not happen

Symptom:

- `update_homebrew_tap` fails or `Formula/tk.rb` on remote stays stale

Fix:

1. Inspect the failed workflow logs
2. Confirm the remote `homebrew-tap` repo contains `Formula/tk.rb`
3. Confirm the `repository_dispatch` payload is accepted by `update-formula.yml`
4. Re-run only after the tap-side issue is fixed

### CI fails on `main`

Symptom:

- tag should not be pushed yet

Fix:

- treat `CI` as the release gate
- fix on `main`, push, and re-watch `CI`

## Practical Notes

- Prefer `gh run watch` over passive waiting.
- Prefer checking exact run IDs rather than assuming the latest run is the right one.
- Prefer reading remote `Formula/tk.rb` after release instead of assuming tap update succeeded.
- If you changed release automation or formula templates, push those changes before tagging.

## References

- Read [references/release-checklist.md](references/release-checklist.md) when you need the compact command checklist and artifact verification list.
