use std::future::Future;
use std::pin::Pin;

use serde::Deserialize;
use serde_json::json;

use super::browser_common::{not_found_or_error, BINARY};
use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// A tool that interacts with browser elements via `agent-browser` actions
/// (click, fill, type, hover, select, check, press).
pub struct BrowserActTool;

#[derive(Debug, Deserialize)]
struct Input {
    action: String,
    #[serde(rename = "ref")]
    element_ref: Option<String>,
    value: Option<String>,
}

/// Actions that require a `value` parameter.
const VALUE_REQUIRED_ACTIONS: &[&str] = &["fill", "type", "select"];

impl Default for BrowserActTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserActTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for BrowserActTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "browser_act".to_string(),
            description: "Interact with a browser element. Use @ref values from browser_snapshot \
                to target elements. Actions: click, fill, type, hover, select, check, press."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["click", "fill", "type", "hover", "select", "check", "press"],
                        "description": "The action to perform"
                    },
                    "ref": {
                        "type": "string",
                        "description": "Element reference from snapshot (e.g. @e1). Required for click/fill/type/hover/select/check."
                    },
                    "value": {
                        "type": "string",
                        "description": "Value for fill/type/select/press actions (required for fill/type/select)"
                    }
                },
                "required": ["action"]
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

            let mut cmd_args: Vec<String> = vec![input.action.clone()];

            // For most actions, a ref is required
            match input.action.as_str() {
                "click" | "fill" | "type" | "hover" | "select" | "check" => {
                    match &input.element_ref {
                        Some(r) => cmd_args.push(r.clone()),
                        None => {
                            return ToolResult::error(format!(
                                "Action '{}' requires a 'ref' parameter (e.g. @e1)",
                                input.action
                            ));
                        }
                    }
                }
                "press" => {
                    // press does not require a ref, just a value (key name)
                }
                _ => {
                    return ToolResult::error(format!(
                        "Unknown action '{}'. Valid actions: click, fill, type, hover, select, check, press",
                        input.action
                    ));
                }
            }

            // Validate that actions requiring a value have one
            if VALUE_REQUIRED_ACTIONS.contains(&input.action.as_str()) && input.value.is_none() {
                return ToolResult::error(format!(
                    "Action '{}' requires a 'value' parameter",
                    input.action
                ));
            }

            // Add value for actions that need it
            if let Some(ref val) = input.value {
                cmd_args.push(val.clone());
            }

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

            let timeout = tokio::time::Duration::from_secs(30);
            match tokio::time::timeout(timeout, child.wait_with_output()).await {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    if output.status.success() {
                        let text = if stdout.trim().is_empty() {
                            format!("Action '{}' completed successfully", input.action)
                        } else {
                            stdout
                        };
                        ToolResult::text(text)
                    } else {
                        ToolResult::error(format!(
                            "agent-browser {} failed (exit {}):\n{stderr}",
                            input.action,
                            output.status.code().unwrap_or(-1)
                        ))
                    }
                }
                Ok(Err(e)) => ToolResult::error(format!("Process error: {e}")),
                Err(_) => ToolResult::error("Execution timed out after 30 seconds"),
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
        let tool = BrowserActTool::new();
        let def = tool.def();
        assert_eq!(def.name, "browser_act");

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["ref"].is_object());
        assert!(schema["properties"]["value"].is_object());
        assert_eq!(schema["required"], json!(["action"]));
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = BrowserActTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_require_ref_for_click() {
        let tool = BrowserActTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({"action": "click"}), &ctx).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn should_require_value_for_fill() {
        let tool = BrowserActTool::new();
        let ctx = test_ctx();
        let result = tool
            .call(json!({"action": "fill", "ref": "@e1"}), &ctx)
            .await;
        assert!(result.is_error);
        let text = result.as_text().unwrap();
        assert!(
            text.contains("requires a 'value'"),
            "Expected value validation error, got: {text}"
        );
    }

    #[tokio::test]
    async fn should_require_value_for_type() {
        let tool = BrowserActTool::new();
        let ctx = test_ctx();
        let result = tool
            .call(json!({"action": "type", "ref": "@e2"}), &ctx)
            .await;
        assert!(result.is_error);
        let text = result.as_text().unwrap();
        assert!(
            text.contains("requires a 'value'"),
            "Expected value validation error, got: {text}"
        );
    }

    #[tokio::test]
    async fn should_require_value_for_select() {
        let tool = BrowserActTool::new();
        let ctx = test_ctx();
        let result = tool
            .call(json!({"action": "select", "ref": "@e3"}), &ctx)
            .await;
        assert!(result.is_error);
        let text = result.as_text().unwrap();
        assert!(
            text.contains("requires a 'value'"),
            "Expected value validation error, got: {text}"
        );
    }

    #[tokio::test]
    async fn should_return_error_when_binary_missing() {
        let tool = BrowserActTool::new();
        let ctx = test_ctx();
        let result = tool
            .call(json!({"action": "click", "ref": "@e1"}), &ctx)
            .await;
        if result.is_error {
            let text = result.as_text().unwrap();
            assert!(
                text.contains("agent-browser") || text.contains("error"),
                "Unexpected error: {text}"
            );
        }
    }
}
