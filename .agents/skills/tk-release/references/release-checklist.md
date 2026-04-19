# TK Release Checklist

Use this as the compact release checklist when following `$tk-release`.

## Preflight

1. Confirm the target version in `Cargo.toml`.
2. Ensure the working tree is clean.
3. Ensure `main` contains:
   - release workflow
   - packaging/homebrew formula template
   - any tap-related changes required for this release
4. Confirm `HOMEBREW_TAP_TOKEN` exists in the `mkusaka/tk` repository secrets.

## Local commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --locked --all-targets
cargo test
cargo package --allow-dirty --locked --no-verify
```

## Push main

```bash
git push origin main
gh run list --limit 10 --json databaseId,workflowName,status,conclusion,headSha
gh run watch <ci-run-id> --exit-status
```

Only continue when `CI` is green for the commit you intend to tag.

## Tag and push

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

Replace `X.Y.Z` with the actual version, and keep it identical to `Cargo.toml`.

## Watch release

```bash
gh run list --workflow Release --limit 10
gh run watch <release-run-id> --exit-status
gh run view <release-run-id> --log-failed
```

## Verify artifacts

```bash
gh release view vX.Y.Z --json isDraft,tagName,url
gh api repos/mkusaka/homebrew-tap/contents/Formula/tk.rb --jq '.download_url'
curl -L "$(gh api repos/mkusaka/homebrew-tap/contents/Formula/tk.rb --jq '.download_url')"
```

## Expected outcomes

- GitHub release exists and is published
- bottle artifacts are attached to the release
- `Formula/tk.rb` in `mkusaka/homebrew-tap` points at the released tag and bottle checksums
