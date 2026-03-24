use std::future::Future;
use std::pin::Pin;

use serde::Deserialize;
use serde_json::json;

use super::browser_common::{not_found_or_error, BINARY};
use crate::{Tool, ToolContext, ToolDef, ToolResult};

const DEFAULT_TIMEOUT_MS: u64 = 30000;

/// A tool that waits for a browser condition via `agent-browser wait <event> [value] [--timeout <ms>]`.
pub struct BrowserWaitTool;

#[derive(Debug, Deserialize)]
struct Input {
    event: String,
    value: Option<String>,
    timeout_ms: Option<u64>,
}

impl Default for BrowserWaitTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserWaitTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for BrowserWaitTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "browser_wait".to_string(),
            description: "Wait for a browser condition. Events: load, network-idle, selector, \
                text, url. Optionally specify a value (selector string, text content, or URL \
                pattern) and timeout."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "event": {
                        "type": "string",
                        "enum": ["load", "network-idle", "selector", "text", "url"],
                        "description": "The event/condition to wait for"
                    },
                    "value": {
                        "type": "string",
                        "description": "Selector string, text content, or URL pattern (required for selector/text/url)"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Timeout in milliseconds (default: 30000)",
                        "default": 30000
                    }
                },
                "required": ["event"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let input: Input = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            // Validate that events requiring a value have one
            match input.event.as_str() {
                "selector" | "text" | "url" => {
                    if input.value.is_none() {
                        return ToolResult::error(format!(
                            "Event '{}' requires a 'value' parameter",
                            input.event
                        ));
                    }
                }
                "load" | "network-idle" => {}
                _ => {
                    return ToolResult::error(format!(
                        "Unknown event '{}'. Valid events: load, network-idle, selector, text, url",
                        input.event
                    ));
                }
            }

            let timeout_ms = input.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

            let mut cmd_args: Vec<String> = vec!["wait".to_string(), input.event.clone()];
            if let Some(ref val) = input.value {
                cmd_args.push(val.clone());
            }
            cmd_args.push("--timeout".to_string());
            cmd_args.push(timeout_ms.to_string());

            let child = match tokio::process::Command::new(BINARY)
                .args(&cmd_args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(c) => c,
                Err(e) => return ToolResult::error(not_found_or_error(e)),
            };

            // Process-level timeout: give extra buffer over the wait timeout
            let process_timeout =
                tokio::time::Duration::from_millis(timeout_ms.saturating_add(5000));
            match tokio::time::timeout(process_timeout, child.wait_with_output()).await {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    if output.status.success() {
                        let text = if stdout.trim().is_empty() {
                            format!("Wait for '{}' completed", input.event)
                        } else {
                            stdout
                        };
                        ToolResult::text(text)
                    } else {
                        ToolResult::error(format!(
                            "agent-browser wait {} failed (exit {}):\n{stderr}",
                            input.event,
                            output.status.code().unwrap_or(-1)
                        ))
                    }
                }
                Ok(Err(e)) => ToolResult::error(format!("Process error: {e}")),
                Err(_) => ToolResult::error(format!("Execution timed out after {timeout_ms}ms")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new("test-agent", "test")
    }

    #[test]
    fn should_have_correct_name_and_schema() {
        let tool = BrowserWaitTool::new();
        let def = tool.def();
        assert_eq!(def.name, "browser_wait");

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["event"].is_object());
        assert!(schema["properties"]["value"].is_object());
        assert!(schema["properties"]["timeout_ms"].is_object());
        assert_eq!(schema["required"], json!(["event"]));
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = BrowserWaitTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_require_value_for_selector_event() {
        let tool = BrowserWaitTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({"event": "selector"}), &ctx).await;
        assert!(result.is_error);
        let text = result.as_text().unwrap();
        assert!(text.contains("requires a 'value'"), "Got: {text}");
    }

    #[tokio::test]
    async fn should_return_error_when_binary_missing() {
        let tool = BrowserWaitTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({"event": "load"}), &ctx).await;
        if result.is_error {
            let text = result.as_text().unwrap();
            assert!(
                text.contains("agent-browser") || text.contains("error"),
                "Unexpected error: {text}"
            );
        }
    }
}
