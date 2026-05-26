# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
