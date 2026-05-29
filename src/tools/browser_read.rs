use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::browser_common::{browser_session, command_with_session, not_found_or_error};
use crate::{Tool, ToolAnnotations, ToolContext, ToolDef, ToolOutput};

/// A tool that reads data from the browser via `agent-browser get <target> [@ref]`.
pub struct BrowserReadTool;

#[derive(Debug, Deserialize)]
struct Input {
    target: String,
    #[serde(rename = "ref")]
    element_ref: Option<String>,
}

impl Default for BrowserReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for BrowserReadTool {
    fn def(&self) -> ToolDef {
        ToolDef::new(
            "browser_read".to_string(),
            "Read data from the browser. Targets: text, html, value (require @ref), \
                url, title (no ref needed)."
                .to_string(),
            json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "enum": ["text", "html", "value", "url", "title"],
                        "description": "What to read from the browser"
                    },
                    "ref": {
                        "type": "string",
                        "description": "Element reference from snapshot (required for text/html/value)"
                    }
                },
                "required": ["target"]
            }),
        )
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            read_only: false,
            destructive: false,
            network_access: true,
            idempotent: false,
        }
    }

    async fn call(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let session = browser_session(ctx);
        let input: Input = match serde_json::from_value(args) {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(format!("Invalid input: {e}")),
        };

        let mut cmd_args: Vec<String> = vec!["get".to_string(), input.target.clone()];

        // text/html/value require a ref
        match input.target.as_str() {
            "text" | "html" | "value" => match &input.element_ref {
                Some(r) => cmd_args.push(r.clone()),
                None => {
                    return ToolOutput::error(format!(
                        "Target '{}' requires a 'ref' parameter (e.g. @e1)",
                        input.target
                    ));
                }
            },
            "url" | "title" => {
                // No ref needed
            }
            _ => {
                return ToolOutput::error(format!(
                    "Unknown target '{}'. Valid targets: text, html, value, url, title",
                    input.target
                ));
            }
        }

        let child = match command_with_session(session.as_deref())
            .args(&cmd_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return ToolOutput::error(not_found_or_error(e)),
        };

        let timeout = tokio::time::Duration::from_secs(30);
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if output.status.success() {
                    ToolOutput::text(stdout)
                } else {
                    ToolOutput::error(format!(
                        "agent-browser get {} failed (exit {}):\n{stderr}",
                        input.target,
                        output.status.code().unwrap_or(-1)
                    ))
                }
            }
            Ok(Err(e)) => ToolOutput::error(format!("Process error: {e}")),
            Err(_) => ToolOutput::error("Execution timed out after 30 seconds"),
        }
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
        let tool = BrowserReadTool::new();
        let def = tool.def();
        assert_eq!(def.name, "browser_read");

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["target"].is_object());
        assert!(schema["properties"]["ref"].is_object());
        assert_eq!(schema["required"], json!(["target"]));
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = BrowserReadTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_require_ref_for_text() {
        let tool = BrowserReadTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({"target": "text"}), &ctx).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn should_return_error_when_binary_missing() {
        let tool = BrowserReadTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({"target": "url"}), &ctx).await;
        if result.is_error {
            let text = result.as_text().unwrap();
            assert!(
                text.contains("agent-browser")
                    || text.contains("error")
                    || text.contains("timed out"),
                "Unexpected error: {text}"
            );
        }
    }
}
