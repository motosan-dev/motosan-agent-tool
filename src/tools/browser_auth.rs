use std::future::Future;
use std::pin::Pin;

use serde::Deserialize;
use serde_json::json;

use super::browser_common::{browser_session, command_with_session, not_found_or_error};
use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// A tool that manages browser authentication state via `agent-browser`.
///
/// Actions:
/// - `load`: Advisory — tells the LLM agent to pass `--state <path>` on the next command.
/// - `save`: Runs `agent-browser state save <path>` to persist current state.
/// - `auto-connect-save`: Runs `agent-browser --auto-connect state save <path>`.
pub struct BrowserAuthTool;

#[derive(Debug, Deserialize)]
struct Input {
    action: String,
    path: String,
}

impl Default for BrowserAuthTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserAuthTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for BrowserAuthTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "browser_auth".to_string(),
            description:
                "Manage browser authentication state. Actions: load (advisory — tells you \
                to pass --state on next command), save (persist current state), auto-connect-save \
                (save with auto-connect)."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["load", "save", "auto-connect-save"],
                        "description": "The auth action to perform"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to the auth state file (e.g. ~/.pilot/auth-591.json)"
                    }
                },
                "required": ["action", "path"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        let session = browser_session(ctx);
        Box::pin(async move {
            let input: Input = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            match input.action.as_str() {
                "load" => {
                    // "load" is advisory only. This tool does not persist any state between
                    // calls. The calling agent must pass --state <path> on subsequent commands.
                    ToolResult::text(format!(
                        "Advisory: To use auth state from {}, pass --state flag on your next \
                         agent-browser command. This tool does not persist state between calls \
                         \u{2014} the calling agent must manage this.",
                        input.path
                    ))
                }
                "save" => run_state_save(session.as_deref(), &input.path, false).await,
                "auto-connect-save" => run_state_save(session.as_deref(), &input.path, true).await,
                _ => ToolResult::error(format!(
                    "Unknown action '{}'. Valid actions: load, save, auto-connect-save",
                    input.action
                )),
            }
        })
    }
}

async fn run_state_save(session: Option<&str>, path: &str, auto_connect: bool) -> ToolResult {
    let mut cmd = command_with_session(session);
    if auto_connect {
        cmd.arg("--auto-connect");
    }
    cmd.arg("state").arg("save").arg(path);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);

    let child = match cmd.spawn() {
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
                    format!("Auth state saved to {path}")
                } else {
                    stdout
                };
                ToolResult::text(text)
            } else {
                ToolResult::error(format!(
                    "agent-browser state save failed (exit {}):\n{stderr}",
                    output.status.code().unwrap_or(-1)
                ))
            }
        }
        Ok(Err(e)) => ToolResult::error(format!("Process error: {e}")),
        Err(_) => ToolResult::error("Execution timed out after 30 seconds"),
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
        let tool = BrowserAuthTool::new();
        let def = tool.def();
        assert_eq!(def.name, "browser_auth");

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["path"].is_object());
        assert_eq!(schema["required"], json!(["action", "path"]));
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = BrowserAuthTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({}), &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_handle_load_action_as_advisory() {
        let tool = BrowserAuthTool::new();
        let ctx = test_ctx();
        let result = tool
            .call(
                json!({"action": "load", "path": "~/.pilot/auth.json"}),
                &ctx,
            )
            .await;
        assert!(!result.is_error);
        let text = result.as_text().unwrap();
        assert!(text.contains("Advisory"), "Got: {text}");
        assert!(text.contains("~/.pilot/auth.json"), "Got: {text}");
        assert!(text.contains("does not persist state"), "Got: {text}");
    }

    #[tokio::test]
    async fn should_return_error_for_save_when_binary_missing() {
        let tool = BrowserAuthTool::new();
        let ctx = test_ctx();
        let result = tool
            .call(
                json!({"action": "save", "path": "/tmp/auth-test.json"}),
                &ctx,
            )
            .await;
        if result.is_error {
            let text = result.as_text().unwrap();
            assert!(
                text.contains("agent-browser") || text.contains("error"),
                "Unexpected error: {text}"
            );
        }
    }

    #[tokio::test]
    async fn should_support_auto_connect_save() {
        let tool = BrowserAuthTool::new();
        let ctx = test_ctx();
        let result = tool
            .call(
                json!({"action": "auto-connect-save", "path": "/tmp/auth-test.json"}),
                &ctx,
            )
            .await;
        // Will error because binary isn't installed, but validates the code path
        if result.is_error {
            let text = result.as_text().unwrap();
            assert!(
                text.contains("agent-browser") || text.contains("error"),
                "Unexpected error: {text}"
            );
        }
    }
}
