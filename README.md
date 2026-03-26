# motosan-agent-tool

[![CI](https://github.com/motosan-dev/motosan-agent-tool/actions/workflows/ci.yml/badge.svg)](https://github.com/motosan-dev/motosan-agent-tool/actions/workflows/ci.yml)

Shared AI agent tool kit — Rust, Python, and TypeScript. Provides the core `Tool` trait, registry, and 11 feature-gated built-in tools used by [motosan-chat](https://github.com/motosan-dev/motosan-chat) and [crucible-agent](https://github.com/daiwanwei/crucible-agent).

## Quick Start

### Rust

```rust
use motosan_agent_tool::{Tool, ToolDef, ToolResult, ToolContext, ToolRegistry};
use serde_json::json;
use std::sync::Arc;

// Implement a tool
struct MyTool;

impl Tool for MyTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "my_tool".into(),
            description: "Does something useful".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let query = args["query"].as_str().unwrap_or("");
            ToolResult::text(format!("Result for: {query}"))
        })
    }
}

// Register and use
#[tokio::main]
async fn main() {
    let registry = ToolRegistry::new();
    registry.register(Arc::new(MyTool)).await;

    let tool = registry.get("my_tool").await.unwrap();
    let ctx = ToolContext::new("user-1", "my-app");
    let result = tool.call(json!({"query": "hello"}), &ctx).await;
    println!("{:?}", result.as_text());
}
```

### Python

```python
from motosan_agent_tool import tool, ToolRegistry

@tool(description="Greet someone")
def greet(name: str) -> str:
    return f"Hello, {name}!"

registry = ToolRegistry()
registry.register(greet)
```

## Design

This crate unifies the tool interfaces of motosan-chat and crucible-agent:

- **`ToolDef`** — schema validation (type checking, enum checking, required fields)
- **`ToolResult`** — typed content (`Text` | `Json`) + optional metadata (`citation`, `duration_ms`)
- **`ToolContext`** — common fields (`caller_id`, `platform`) + extensible `extra` map
- **`ToolRegistry`** — thread-safe async tool storage

## Built-in Tools (Rust)

All tools are feature-gated. Enable individually or use `all_tools` to enable all.

| Tool | Feature | Description |
|------|---------|-------------|
| `WebSearchTool` | `web_search` | Brave Search API integration |
| `FetchUrlTool` | `fetch_url` | HTTP fetch with HTML extraction + SSRF protection |
| `ReadFileTool` | `read_file` | Local file reader with path traversal protection |
| `ReadPdfTool` | `read_pdf` | PDF text extraction (local files and URLs) |
| `ReadSpreadsheetTool` | `read_spreadsheet` | Excel (.xlsx/.xls) and CSV reader |
| `JsEvalTool` | `js_eval` | Sandboxed JS evaluation via Boa Engine |
| `PythonEvalTool` | `python_eval` | Python subprocess execution with timeout |
| `DatetimeTool` | `datetime` | Current time, date arithmetic, date diff with timezone support |
| `CurrencyConvertTool` | `currency_convert` | Live exchange rates with caching and API fallback |
| `CostCalculatorTool` | `cost_calculator` | Multi-currency cost breakdown with auto conversion |
| `GeneratePdfTool` | `generate_pdf` | Generate PDF from text/Markdown |

```toml
# Enable specific tools
[dependencies]
motosan-agent-tool = { version = "0.2", features = ["datetime", "web_search"] }

# Or enable all
motosan-agent-tool = { version = "0.2", features = ["all_tools"] }
```

## Multi-language Support

| Package | Language | Install |
|---------|----------|---------|
| `motosan-agent-tool` | Rust | `cargo add motosan-agent-tool` |
| `motosan-agent-tool` | Python (≥3.9) | `pip install motosan-agent-tool` |
| `motosan-agent-tool` | TypeScript | `npm install motosan-agent-tool` |

All three packages share the same API surface: `Tool`, `ToolDef`, `ToolResult`, `ToolContent`, `ToolContext`, `ToolRegistry`, `ToolError`.

The Python package also provides `FunctionTool` and a `@tool` decorator for defining tools from plain functions.

## License

MIT
