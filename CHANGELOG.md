# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## 0.6.0 — 2026-05-29

BREAKING:
- `ToolDef` now composes `motosan_agent_primitives::ToolSchema` via
  `#[serde(flatten)]`; the serialized wire shape remains unchanged.
  Field reads (`def.name`, `def.description`, `def.input_schema`) continue
  to work through `Deref<Target = ToolSchema>`, but struct-literal callers
  must wrap those fields in `schema: ToolSchema { .. }`.

ADDED:
- `Deref<Target = ToolSchema>` for `ToolDef`.

DEPS:
- `motosan-agent-primitives` path-dep version pin bumped 0.2.0 → 0.3.0.

## 0.5.0 — 2026-05-29

M10 D-M10-4 — Tool display name distinct from internal name. Resolves
FinanceHarness AWKWARDNESS #4 (M9 consumer feedback): the docs recommended
namespaced names like `finance.place_order`, but Anthropic / OpenAI tool
calling APIs are stricter on naming and LLM prompt clarity favors short
unqualified names. Finance harness ended up using `place_order` directly,
conflicting with the docs.

BREAKING:
- `ToolDef` gains a new `pub internal_name: String` field. The public
  `name` stays unqualified (what the LLM sees); `internal_name` is the
  host-side identifier used for collision detection across stacked
  harnesses and audit correlation. Free-form String — consumers pick
  dotted (`finance.place_order`), slashed, or any other format.
- Existing code that constructs `ToolDef` via struct literal must migrate
  to `ToolDef::new(name, description, input_schema)` (or set the new field
  explicitly). All 19 in-tree built-in tools migrated mechanically — no
  semantic change.
- `ToolDef` now has a manual `Deserialize` impl (was derive). Legacy
  JSON payloads without an `internal_name` field continue to deserialize
  cleanly (`internal_name` defaults to a clone of `name`). `Serialize`
  remains derived — the field is always emitted on the wire so audit /
  snapshot consumers persist the full shape. Existing serde round-trip
  test still passes.

ADDED:
- `ToolDef::new(name, description, input_schema)` constructor.
  `internal_name` defaults to `name.clone()`.
- `ToolDef::with_internal_name(impl Into<String>) -> Self` builder for
  setting a host-side identifier distinct from the LLM-facing `name`.
  Will be used by `motosan-agent-harness-finance` 0.2.0 in M10 Phase F.

DEPS:
- `motosan-agent-primitives` path-dep version pin bumped 0.1.1 → 0.2.0
  (M10 Phase A shipped primitives 0.2.0). No primitives API surface used
  by this crate changed in 0.2.0 — additive field changes on hook ctx
  structs that `motosan-agent-tool` does not consume.

NOTES:
- Tool collision / uniqueness logic in `motosan-agent-loop` (which keys
  by `name` today) is NOT changed in this release. Phase C of M10
  migrates the loop's uniqueness check to use `internal_name`.
- None of the 19 built-in tools sets `internal_name` explicitly — they
  all default. The finance harness will namespace its tools via
  `.with_internal_name("finance.{tool}")` in Phase F.

## 0.4.0 — 2026-05-26

BREAKING:
- Tool trait uses #[async_trait]; signature changed from manual Pin<Box<Future>> to async fn call(...).
- Tool::annotations() is now mandatory (no default impl). Tool authors must declare ToolAnnotations explicitly.
- ToolResult removed from this crate. Use motosan_agent_primitives::ToolResult on the wire and the new ToolOutput type for in-crate tool returns.
- ToolContent removed. Use motosan_agent_primitives::ContentBlock.
- ToolContext gained a cancellation_token field (tokio_util::sync::CancellationToken); marked #[serde(skip, default)] for wire-format back-compat.

ADDED:
- ToolOutput struct with content/is_error/citation/inject_to_context/duration_ms fields and an into_tool_result(tool_use_id) conversion to the primitives wire type.
- Re-exports from motosan-agent-primitives: ContentBlock, ToolAnnotations, ToolCall, ToolResult.

DEPS:
- New: motosan-agent-primitives, async-trait, tokio-util.

## [Unreleased]

### Added
- **`WebSearchTool` Tavily support** (#29): `WebSearchTool` now supports Tavily Search API alongside Brave. Set `TAVILY_API_KEY` to use Tavily (preferred when both keys are present), or `BRAVE_API_KEY` for Brave. Use `SEARCH_PROVIDER=tavily|brave` (case-insensitive) to force a specific provider. Provider-specific error messages when a key is missing for the requested provider.

## [0.3.2] — 2026-04-04

### Added
- **`ToolContext::cwd`** (#37): typed `Option<PathBuf>` field for per-call working directory override. Replaces the untyped `extra["cwd"]` pattern with a first-class, discoverable API.
- `ToolContext::with_cwd()` builder method to set the working directory.
- `ReadFileTool`, `ReadPdfTool`, `ReadSpreadsheetTool`, and `GeneratePdfTool` all resolve relative paths against `ctx.cwd` when set; absolute paths and URLs are unchanged.
- TypeScript `ToolContext.withCwd(path)` and `cwd?: string` field (package 0.2.3).
- Python `ToolContext.with_cwd(path)` and `cwd: Path | None` field (package 0.2.3).

## [0.3.0] — 2026-03-28

### Added
- **ToolContext-based browser session isolation** (#34): All browser tools now read `ctx.get_str("browser_session")` and inject `--session-name <value>` into `agent-browser` commands. Enables thread-safe parallel browser execution when the caller sets `browser_session` in ToolContext.
- `browser_common::command_with_session()` — builds `agent-browser` Command with optional session name
- `browser_common::browser_session()` — extracts session from ToolContext for async-safe usage

## [0.2.2] — 2026-03-26

### Added
- **Browser tools** — 7 tools powered by `agent-browser` CLI (feature: `browser`):
  - `BrowserNavigateTool` — open URLs with validation
  - `BrowserActTool` — click, fill, type, hover, select, check, press
  - `BrowserReadTool` — read text, HTML, attributes from elements
  - `BrowserSnapshotTool` — capture accessibility tree snapshot
  - `BrowserScreenshotTool` — take page screenshots
  - `BrowserWaitTool` — wait for navigation, selector, or network idle
  - `BrowserAuthTool` — save/load authentication state

### Changed
- README: added built-in tools table (18 tools), multi-language support section, Python quick start
- Aligned TypeScript package version to 0.2.2

## [0.2.1] — 2026-03-26

### Added
- **DatetimeTool** built-in — `get_current_datetime`, `date_add`, `date_diff` with timezone support (feature: `datetime`)
- **CurrencyConvertTool** — live exchange rates via free APIs with 1-hour cache and automatic fallback (feature: `currency_convert`)
- **CostCalculatorTool** — multi-currency cost breakdown with automatic conversion (feature: `cost_calculator`)
- **GeneratePdfTool** — generate PDF files from plain text or basic Markdown with path traversal protection (feature: `generate_pdf`)
- **Python `FunctionTool`** class and `@tool` decorator for defining tools from plain functions
- **Python `DatetimeTool`** built-in (mirrors Rust API)
- Release metadata in `pyproject.toml` and `package.json`

### Fixed
- Python 3.9 compatibility — replaced PEP 604 unions with `typing.Union`
- Added `tokio/time` feature to `js_eval` and `python_eval` features

## [0.2.0] — 2026-03-16

### Added
- **Python package** (`python/`): Pure Python, zero runtime deps, mirrors Rust API
- **TypeScript package** (`typescript/`): Strict TypeScript, ESM+CJS, mirrors Rust API (camelCase)
- Both packages: Tool, ToolDef, ToolResult, ToolContent, ToolContext, ToolRegistry, ToolError

## [0.1.2] — 2026-03-16

### Added
- `src/tools/` module with 7 feature-gated built-in tools:
  - `web_search` — Brave Search API integration (feature: `web_search`)
  - `fetch_url` — HTTP fetch with HTML extraction and SSRF protection (feature: `fetch_url`)
  - `read_file` — Local file reader with path traversal protection (feature: `read_file`)
  - `read_pdf` — PDF text extraction via pdf-extract, supports local files and URLs (feature: `read_pdf`)
  - `read_spreadsheet` — Excel (.xlsx/.xls) and CSV reader via calamine (feature: `read_spreadsheet`)
  - `js_eval` — Sandboxed JavaScript evaluation via Boa Engine with built-in helpers (feature: `js_eval`)
  - `python_eval` — Python subprocess execution with timeout (feature: `python_eval`)
- `all_tools` meta-feature to enable all built-in tools
- Each tool includes comprehensive unit tests (52 new tests, 79 total)
- SSRF protection shared across `fetch_url` and `read_pdf` (blocks private/reserved IPs)
- JS eval includes statistical helpers: csv(), sum(), avg(), median(), stdev(), percentile(), groupBy(), sortBy()

## [0.1.1] — 2026-03-16

### Added
- Serialize/Deserialize for all public types (`ToolDef`, `ToolResult`, `ToolContent`, `ToolContext`)
- `ToolContent` uses internally tagged enum (`{"type": "text", "data": "..."}`)
- `ToolRegistry::deregister()` to remove a tool by name
- `ToolRegistry::clear()` to remove all tools
- `Error` conversions: `From<std::io::Error>`, `From<serde_json::Error>`, `From<String>`, `From<&str>`
- GitHub Actions CI (test + clippy + fmt)

## [0.1.0] — 2026-03-16

### Added
- Initial release
- `Tool` trait with async `call()` and `def()`
- `ToolDef` with input schema validation (type checking, enum checking, required fields)
- `ToolResult` with typed content (`Text` | `Json`) and optional metadata (`citation`, `duration_ms`)
- `ToolContext` with common fields (`caller_id`, `platform`) and extensible `extra` map
- `ToolRegistry` — thread-safe async tool storage
- `ToolDef::parse_args()` for typed deserialization with validation
