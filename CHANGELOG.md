# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
