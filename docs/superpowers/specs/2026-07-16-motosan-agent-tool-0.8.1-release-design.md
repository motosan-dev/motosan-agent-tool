# Motosan Agent Tool 0.8.1 Release Design

## Context

`motosan-agent-loop` 0.49.0 requires `motosan-agent-tool` 0.8.1, but
crates.io currently stops at 0.8.0. The 0.8.1 source is already merged at
`790699abf210aa8c10bdd52d506c0031eeff5cb1`. Its local package, clippy, and
test gates pass when `motosan-agent-primitives` 0.4.0 is available at the
declared sibling path. GitHub CI fails before compilation because it does not
provision that sibling.

## Decision

Keep the existing path-plus-version dependency:

```toml
motosan-agent-primitives = { path = "../motosan-agent-primitives", version = "0.4.0" }
```

This preserves the sibling-repository development workflow while allowing
Cargo to normalize the published package to the registry dependency. Each CI
job downloads the immutable published 0.4.0 source into the expected sibling
path before invoking Cargo.

Release metadata is corrected on the same reviewed release-prep branch:

- README dependency examples use the current 0.8 line;
- the 0.8.1 changelog date matches the actual release date;
- `.DS_Store` is ignored and excluded from the Rust package; and
- `docs/superpowers/**` is excluded from the Rust package.

No Rust runtime behavior changes in this release-prep train.

## Release Gate

The exact release commit must pass:

```text
cargo fmt -- --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
cargo package --locked
cargo publish --dry-run --locked
```

After the release-prep PR merges and its GitHub checks pass, publish from a
clean worktree pinned to the merged commit. Verify crates.io exposes 0.8.1
before creating and pushing annotated tag `v0.8.1` on that same commit.

The registry upload and tag are irreversible release actions and were
explicitly authorized by the user on 2026-07-16.
