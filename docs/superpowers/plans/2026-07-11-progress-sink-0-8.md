# ProgressSink + ToolContext.progress (tool 0.8.0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ProgressSink` (a neutral, engine-agnostic streaming handle) and a `ToolContext.progress` field so tools can emit incremental output — the tool-crate half of the W1 ToolOutputDelta design (spec: `motosan-agent-loop/docs/superpowers/specs/2026-07-11-w1-tool-output-delta-design.md`).

**Architecture:** `ProgressSink` is a cheap cloneable wrapper over `Option<Arc<dyn Fn(String)>>` — default inactive (emit = no-op), engines construct active sinks. It rides `ToolContext` as a `#[serde(skip, default)]` runtime-only field, exactly like `cancellation_token`. Wire format is unchanged (serde skip), so the python/ and typescript/ bindings are untouched.

**Tech Stack:** Rust. No new dependencies. Base: motosan-agent-tool 0.7.0.

## Global Constraints

- Verify LOCALLY: `cargo fmt --all -- --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo test`, `cargo test --all-features` (features exist: web_search/fetch_url/read_file/read_pdf/read_spreadsheet).
- This release is **0.8.0 (breaking)**: exhaustive struct literals of `ToolContext` gain a field. The ONLY breaking change is that field; do not touch the `Tool` trait, `ToolOutput`, `ToolDef`, or the wire-format types.
- `ProgressSink` stays engine-agnostic: no new deps, no engine/loop types, no async.
- Do NOT touch `python/` or `typescript/` (serde skip keeps the wire format identical).
- Every commit message ends with: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- Do NOT run `cargo publish`.

---

### Task 1: `ProgressSink` type

**Files:**
- Modify: `src/tool.rs` (type above the `ToolContext` section, ~line 455), `src/lib.rs` (export)

**Interfaces:**
- Produces: `ProgressSink` (`Clone + Default + Debug`) with `new(f: impl Fn(String) + Send + Sync + 'static) -> Self`, `emit(&self, chunk: impl Into<String>)`, `is_active(&self) -> bool`. Task 2 embeds it in `ToolContext`; the loop repo's 0.42 plan consumes it via `motosan_agent_tool::ProgressSink`.

- [ ] **Step 1: Write the failing tests** (append a new module at the bottom of `src/tool.rs`)

```rust
#[cfg(test)]
mod progress_sink_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn default_is_inactive_and_emit_is_noop() {
        let sink = ProgressSink::default();
        assert!(!sink.is_active());
        sink.emit("dropped"); // must not panic
        assert_eq!(format!("{sink:?}"), "ProgressSink(inactive)");
    }

    #[test]
    fn active_sink_receives_chunks_in_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_c = Arc::clone(&seen);
        let sink = ProgressSink::new(move |c| seen_c.lock().unwrap().push(c));
        assert!(sink.is_active());
        assert_eq!(format!("{sink:?}"), "ProgressSink(active)");
        sink.emit("one");
        sink.emit(String::from("two"));
        assert_eq!(
            *seen.lock().unwrap(),
            vec!["one".to_string(), "two".to_string()]
        );
    }

    #[test]
    fn clones_share_the_consumer() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_c = Arc::clone(&seen);
        let sink = ProgressSink::new(move |c| seen_c.lock().unwrap().push(c));
        let clone = sink.clone();
        clone.emit("via-clone");
        assert_eq!(*seen.lock().unwrap(), vec!["via-clone".to_string()]);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test progress_sink`
Expected: FAIL to compile — `ProgressSink` does not exist.

- [ ] **Step 3: Implement** — in `src/tool.rs`, directly above the `ToolContext` doc block:

```rust
// ---------------------------------------------------------------------------
// ProgressSink
// ---------------------------------------------------------------------------

/// Cheap cloneable handle for streaming incremental tool output.
///
/// Default = inactive: [`ProgressSink::emit`] is a no-op. Engines construct
/// active sinks bound to their own event plumbing via [`ProgressSink::new`];
/// tools call `emit` with text chunks (granularity is the tool's choice —
/// typically a line or a read buffer). Tools doing expensive formatting
/// solely for progress output should gate on [`ProgressSink::is_active`].
///
/// Neutral type by design: no engine dependencies, mirroring how
/// [`CancellationToken`] rides [`ToolContext`].
#[derive(Clone, Default)]
pub struct ProgressSink(Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>);

impl ProgressSink {
    /// An active sink that invokes `f` for every emitted chunk.
    pub fn new(f: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self(Some(std::sync::Arc::new(f)))
    }

    /// Emit one chunk of incremental output. No-op when inactive.
    pub fn emit(&self, chunk: impl Into<String>) {
        if let Some(f) = &self.0 {
            f(chunk.into());
        }
    }

    /// `true` when an engine attached a consumer.
    pub fn is_active(&self) -> bool {
        self.0.is_some()
    }
}

impl std::fmt::Debug for ProgressSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.is_active() {
            "ProgressSink(active)"
        } else {
            "ProgressSink(inactive)"
        })
    }
}
```

In `src/lib.rs`, extend the tool export line:

```rust
pub use tool::{ProgressSink, Tool, ToolContext, ToolDef, ToolOutput};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test progress_sink`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit** (include this plan file — first commit of the train)

```bash
git add src/tool.rs src/lib.rs docs/superpowers/plans/2026-07-11-progress-sink-0-8.md
git commit -m "feat: ProgressSink — neutral streaming handle for incremental tool output"
```

---

### Task 2: `ToolContext.progress` field

**Files:**
- Modify: `src/tool.rs` (`ToolContext` struct ~line 473, `ToolContext::new` ~line 498, builder methods block)

**Interfaces:**
- Consumes: Task 1's `ProgressSink`.
- Produces: `pub progress: ProgressSink` field (`#[serde(skip, default)]`), `ToolContext::with_progress(self, ProgressSink) -> Self` builder. The loop's 0.42 plan sets the field directly on a per-call clone.

- [ ] **Step 1: Write the failing tests** (append to the `progress_sink_tests` module from Task 1)

```rust
    #[test]
    fn tool_context_default_carries_inactive_sink() {
        let ctx = ToolContext::default();
        assert!(!ctx.progress.is_active());
        let ctx2 = ToolContext::new("caller", "platform");
        assert!(!ctx2.progress.is_active());
    }

    #[test]
    fn tool_context_serde_round_trip_yields_inactive_sink() {
        let ctx = ToolContext::new("caller", "platform")
            .with_progress(ProgressSink::new(|_| {}));
        assert!(ctx.progress.is_active());
        let json = serde_json::to_string(&ctx).expect("serialize");
        let back: ToolContext = serde_json::from_str(&json).expect("deserialize");
        assert!(
            !back.progress.is_active(),
            "serde(skip) must yield the default inactive sink"
        );
        assert_eq!(back.caller_id, "caller");
        assert_eq!(back.platform, "platform");
    }

    #[test]
    fn with_progress_attaches_active_sink() {
        let ctx = ToolContext::new("c", "p").with_progress(ProgressSink::new(|_| {}));
        assert!(ctx.progress.is_active());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test progress_sink`
Expected: FAIL to compile — no `progress` field / `with_progress`.

- [ ] **Step 3: Implement**

Add to the `ToolContext` struct, after `cancellation_token`:

```rust
    /// Sink for streaming incremental output while the call runs. Engines
    /// attach an active sink per call; the default is inactive (`emit` is a
    /// no-op). `#[serde(skip)]` — like `cancellation_token`, a deserialized
    /// context gets the default (inactive) sink.
    #[serde(skip, default)]
    pub progress: ProgressSink,
```

In `ToolContext::new`, add `progress: ProgressSink::default(),` to the literal. Next to `with_cancellation`, add:

```rust
    /// Attach an engine-provided progress sink (builder pattern).
    pub fn with_progress(mut self, sink: ProgressSink) -> Self {
        self.progress = sink;
        self
    }
```

Then sweep for other exhaustive `ToolContext {` literals: `grep -rn "ToolContext {" src/` — add the field (or `..Default::default()`) to each.

- [ ] **Step 4: Run the full gates**

Run: `cargo test && cargo test --all-features && cargo clippy --all-features --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tool.rs
git commit -m "feat!: ToolContext.progress — per-call streaming sink field"
```

---

### Task 3: Release housekeeping 0.8.0

**Files:**
- Modify: `Cargo.toml:3` (`version = "0.8.0"`), `CHANGELOG.md`, `README.md` (only if it shows `ToolContext` struct literals)

- [ ] **Step 1: `CHANGELOG.md`** — new section at the top, matching the existing `## 0.7.0 — 2026-06-02` heading style (use today's date):

```markdown
## 0.8.0 — 2026-07-XX

### Added
- `ProgressSink` — neutral, cloneable handle for streaming incremental tool
  output. Default is inactive (`emit` is a no-op); engines construct active
  sinks. Exported from the crate root.
- `ToolContext.progress: ProgressSink` (`#[serde(skip, default)]`) and
  `ToolContext::with_progress(..)`. Tools stream by calling
  `ctx.progress.emit("chunk")`; gate expensive formatting on
  `ctx.progress.is_active()`.

### Breaking
- Exhaustive struct literals of `ToolContext` must add
  `progress: ProgressSink::default(),` (or switch to `ToolContext::new(..)` /
  `..Default::default()`). Serde wire format is UNCHANGED (the field is
  skipped), so persisted contexts and the python/typescript bindings are
  unaffected.
```

- [ ] **Step 2: README sweep**

Run: `grep -n "ToolContext {" README.md`
Update any exhaustive literal shown in examples (add the field or use `..Default::default()`); if no hits, nothing to do.

- [ ] **Step 3: Full gates**

Run, expecting every command to PASS:

```bash
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test
cargo test --all-features
```

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml CHANGELOG.md README.md
git commit -m "release: 0.8.0 — ProgressSink + ToolContext.progress"
```

---

## Self-Review Notes

- Scope: exactly the tool-crate half of the W1 spec — no Tool-trait changes, no MCP, no bindings work.
- `ProgressSink` derives satisfy `ToolContext`'s existing `derive(Debug, Clone, Default, Serialize, Deserialize)` row: Debug (manual impl), Clone, Default all present; serde never sees the field.
- The loop repo's companion plan (`motosan-agent-loop/docs/superpowers/plans/2026-07-11-w1-tool-output-delta-0-42.md`) consumes exactly: `motosan_agent_tool::ProgressSink::new(..)` + the public `progress` field. Publish order at release time: this crate first.
