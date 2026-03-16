# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
