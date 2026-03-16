# motosan-agent-tool

[![CI](https://github.com/motosan-dev/motosan-agent-tool/actions/workflows/ci.yml/badge.svg)](https://github.com/motosan-dev/motosan-agent-tool/actions/workflows/ci.yml)

Shared AI agent tool kit for Rust. Provides the core `Tool` trait, registry, and built-in tools used by [motosan-chat](https://github.com/motosan-dev/motosan-chat) and [crucible-agent](https://github.com/daiwanwei/crucible-agent).

## Quick Start

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

## Design

This crate unifies the tool interfaces of motosan-chat and crucible-agent:

- **`ToolDef`** — schema validation (type checking, enum checking, required fields)
- **`ToolResult`** — typed content (`Text` | `Json`) + optional metadata (`citation`, `duration_ms`)
- **`ToolContext`** — common fields (`caller_id`, `platform`) + extensible `extra` map
- **`ToolRegistry`** — thread-safe async tool storage

## License

MIT
