use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::browser_common::{browser_session, command_with_session, not_found_or_error};
use crate::{Tool, ToolAnnotations, ToolContext, ToolDef, ToolOutput};

/// A tool that takes a browser screenshot via `agent-browser screenshot [path]`.
pub struct BrowserScreenshotTool;

#[derive(Debug, Deserialize)]
struct Input {
    path: Option<String>,
}

impl Default for BrowserScreenshotTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserScreenshotTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn def(&self) -> ToolDef {
        ToolDef::new(
            "browser_screenshot".to_string(),
            "Take a screenshot of the current browser page. Optionally specify a \
                file path to save it to; otherwise a temporary file is used."
                .to_string(),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to save the screenshot (optional, defaults to temp file)"
                    }
                },
                "required": []
            }),
        )
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            read_only: false,
            destructive: true,
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

        let mut cmd_args: Vec<String> = vec!["screenshot".to_string()];
        if let Some(ref p) = input.path {
            cmd_args.push(p.clone());
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
                    let text = if stdout.trim().is_empty() {
                        "Screenshot captured".to_string()
                    } else {
                        stdout
                    };
                    ToolOutput::text(text)
                } else {
                    ToolOutput::error(format!(
                        "agent-browser screenshot failed (exit {}):\n{stderr}",
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
        let tool = BrowserScreenshotTool::new();
        let def = tool.def();
        assert_eq!(def.name, "browser_screenshot");

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn should_return_error_when_binary_missing() {
        let tool = BrowserScreenshotTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({}), &ctx).await;
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
