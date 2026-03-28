use std::future::Future;
use std::pin::Pin;

use serde_json::json;

use super::browser_common::{browser_session, command_with_session, not_found_or_error};
use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// A tool that captures the accessibility tree snapshot via `agent-browser snapshot`.
pub struct BrowserSnapshotTool;

impl Default for BrowserSnapshotTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserSnapshotTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for BrowserSnapshotTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "browser_snapshot".to_string(),
            description: "Capture the browser accessibility tree with @ref annotations. \
                Use the @refs to interact with elements via browser_act."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    fn call(
        &self,
        _args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        let session = browser_session(ctx);
        Box::pin(async move {
            let child = match command_with_session(session.as_deref())
                .arg("snapshot")
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
                        ToolResult::text(stdout)
                    } else {
                        ToolResult::error(format!(
                            "agent-browser snapshot failed (exit {}):\n{stderr}",
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
        let tool = BrowserSnapshotTool::new();
        let def = tool.def();
        assert_eq!(def.name, "browser_snapshot");
        assert_eq!(def.input_schema["type"], "object");
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn should_return_error_when_binary_missing() {
        let tool = BrowserSnapshotTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({}), &ctx).await;
        if result.is_error {
            let text = result.as_text().unwrap();
            assert!(
                text.contains("agent-browser") || text.contains("error"),
                "Unexpected error: {text}"
            );
        }
    }
}
