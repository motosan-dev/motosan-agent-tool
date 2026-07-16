# Motosan Agent Tool 0.8.1 Release Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development
> (if subagents are available) or superpowers:executing-plans to implement this
> plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce, publish, and tag a reviewed `motosan-agent-tool` 0.8.1
artifact that unblocks `motosan-agent-loop` 0.49.0 CI and publication.

**Architecture:** Preserve the existing path-plus-version dependency for local
sibling development. Provision the published primitives source in CI, correct
release metadata, and publish only from the exact green merge commit.

**Tech Stack:** Rust/Cargo, GitHub Actions, crates.io, Git worktrees.

---

## Task 1: Freeze the release contract

**Files:**
- Create: `docs/superpowers/specs/2026-07-16-motosan-agent-tool-0.8.1-release-design.md`
- Create: `docs/superpowers/plans/2026-07-16-motosan-agent-tool-0.8.1-release.md`

- [ ] Review the design and implementation plan against issue `#42`.
- [ ] Confirm the branch is based on
      `790699abf210aa8c10bdd52d506c0031eeff5cb1`.
- [ ] Commit only the two planning files:

```bash
git add \
  docs/superpowers/specs/2026-07-16-motosan-agent-tool-0.8.1-release-design.md \
  docs/superpowers/plans/2026-07-16-motosan-agent-tool-0.8.1-release.md
git commit -m "feat: plan tool 0.8.1 release (#42)"
```

## Task 2: Repair CI sibling provisioning

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] Run a failing structural assertion:

```bash
python3 - <<'PY'
from pathlib import Path
text = Path(".github/workflows/ci.yml").read_text()
assert text.count("fetch motosan-agent-primitives 0.4.0") == 3
PY
```

Expected before the fix: assertion failure because CI provisions zero sibling
copies.

- [ ] Add the same `fetch` helper and
      `fetch motosan-agent-primitives 0.4.0` call before Cargo setup in the
      `test`, `clippy`, and `fmt` jobs.
- [ ] Re-run the structural assertion and parse the YAML.
- [ ] Do not change toolchain selection or Cargo commands.

## Task 3: Correct release metadata and package scope

**Files:**
- Modify: `.gitignore`
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] Run a failing metadata assertion:

```bash
python3 - <<'PY'
from pathlib import Path
cargo = Path("Cargo.toml").read_text()
readme = Path("README.md").read_text()
changelog = Path("CHANGELOG.md").read_text()
ignore = Path(".gitignore").read_text()
assert 'exclude = ["**/.DS_Store", "docs/superpowers/**"]' in cargo
assert 'version = "0.8"' in readme
assert "## 0.8.1 — 2026-07-16" in changelog
assert "**/.DS_Store" in ignore
PY
```

Expected before the fix: assertion failure.

- [ ] Add the exact package exclusions, update both README dependency examples
      to `0.8`, update the changelog date, and ignore `.DS_Store` recursively.
- [ ] Re-run the metadata assertion.
- [ ] Run `cargo package --locked --list` and verify neither `.DS_Store` nor
      `docs/superpowers/` appears.

## Task 4: Verify and commit release preparation

- [ ] Run:

```bash
cargo fmt -- --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
cargo package --locked
cargo publish --dry-run --locked
git diff --check
```

- [ ] Dispatch specification and code-quality reviews. Resolve every finding
      and re-run the full gate.
- [ ] Stage only:

```bash
git add \
  .github/workflows/ci.yml \
  .gitignore \
  Cargo.toml \
  README.md \
  CHANGELOG.md
git commit -m "fix: prepare tool 0.8.1 release (#42)"
```

## Task 5: Merge the exact release commit

- [ ] Push only to `origin`.
- [ ] Create a PR to `main` linked to issue `#42`.
- [ ] Wait for all GitHub checks to pass.
- [ ] Merge without force-pushing.
- [ ] Fetch `origin/main` and record the exact merge commit.

## Task 6: Publish and tag

- [ ] Create or reuse a clean worktree pinned to the exact merged commit.
- [ ] Re-run the full Task 4 gate.
- [ ] Verify the authenticated crates.io owner:

```bash
cargo owner --list motosan-agent-tool
```

- [ ] Publish:

```bash
cargo publish --locked
```

- [ ] Poll the crates.io API until version 0.8.1 is visible.
- [ ] Create and push the annotated tag on the same commit:

```bash
git tag -a v0.8.1 -m "motosan-agent-tool 0.8.1"
git push origin v0.8.1
```

- [ ] Confirm the registry checksum/version and remote tag commit agree with
      the release evidence.
