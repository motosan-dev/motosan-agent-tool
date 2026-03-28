use std::future::Future;
use std::pin::Pin;

use serde::Deserialize;
use serde_json::json;

use super::browser_common::{browser_session, command_with_session, not_found_or_error};
use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// A tool that manages browser tabs: open, list, switch, and close.
pub struct BrowserTabTool;

#[derive(Debug, Deserialize)]
struct Input {
    action: String,
    index: Option<u32>,
    url: Option<String>,
}

impl Default for BrowserTabTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserTabTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for BrowserTabTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "browser_tab".to_string(),
            description:
                "Manage browser tabs: open new tab, switch between tabs, list tabs, close tab."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["new", "list", "switch", "close"],
                        "description": "Tab action: 'new' opens a new tab, 'list' shows all tabs, 'switch' activates a tab by index, 'close' closes current or specified tab"
                    },
                    "index": {
                        "type": "integer",
                        "description": "Tab index (for 'switch' and 'close' actions)"
                    },
                    "url": {
                        "type": "string",
                        "description": "URL to open in the new tab (optional, only for 'new' action)"
                    }
                },
                "required": ["action"]
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

            let session_ref = session.as_deref();

            match input.action.as_str() {
                "new" => {
                    let child = match command_with_session(session_ref)
                        .args(["tab", "new"])
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .kill_on_drop(true)
                        .spawn()
                    {
                        Ok(c) => c,
                        Err(e) => return ToolResult::error(not_found_or_error(e)),
                    };

                    let output = match tokio::time::timeout(
                        tokio::time::Duration::from_secs(10),
                        child.wait_with_output(),
                    )
                    .await
                    {
                        Ok(Ok(o)) => o,
                        Ok(Err(e)) => return ToolResult::error(format!("Process error: {e}")),
                        Err(_) => return ToolResult::error("Tab new timed out"),
                    };

                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        return ToolResult::error(format!("tab new failed: {stderr}"));
                    }

                    // If URL provided, navigate in the new tab
                    if let Some(ref url) = input.url {
                        let nav_child = match command_with_session(session_ref)
                            .args(["open", url])
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .kill_on_drop(true)
                            .spawn()
                        {
                            Ok(c) => c,
                            Err(e) => return ToolResult::error(not_found_or_error(e)),
                        };

                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(30),
                            nav_child.wait_with_output(),
                        )
                        .await
                        {
                            Ok(Ok(o)) if o.status.success() => {}
                            Ok(Ok(o)) => {
                                let stderr = String::from_utf8_lossy(&o.stderr);
                                return ToolResult::error(format!("navigate failed: {stderr}"));
                            }
                            Ok(Err(e)) => return ToolResult::error(format!("Process error: {e}")),
                            Err(_) => return ToolResult::error("Navigation timed out"),
                        }

                        ToolResult::text(format!("Opened new tab and navigated to {url}"))
                    } else {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        ToolResult::text(if stdout.trim().is_empty() {
                            "Opened new tab".to_string()
                        } else {
                            stdout.to_string()
                        })
                    }
                }
                "list" => {
                    let child = match command_with_session(session_ref)
                        .args(["tab", "list"])
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .kill_on_drop(true)
                        .spawn()
                    {
                        Ok(c) => c,
                        Err(e) => return ToolResult::error(not_found_or_error(e)),
                    };

                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(10),
                        child.wait_with_output(),
                    )
                    .await
                    {
                        Ok(Ok(o)) if o.status.success() => {
                            let stdout = String::from_utf8_lossy(&o.stdout);
                            ToolResult::text(stdout.to_string())
                        }
                        Ok(Ok(o)) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            ToolResult::error(format!("tab list failed: {stderr}"))
                        }
                        Ok(Err(e)) => ToolResult::error(format!("Process error: {e}")),
                        Err(_) => ToolResult::error("Tab list timed out"),
                    }
                }
                "switch" => {
                    let idx = match input.index {
                        Some(i) => i.to_string(),
                        None => {
                            return ToolResult::error("'switch' action requires 'index' parameter")
                        }
                    };

                    let child = match command_with_session(session_ref)
                        .args(["tab", &idx])
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .kill_on_drop(true)
                        .spawn()
                    {
                        Ok(c) => c,
                        Err(e) => return ToolResult::error(not_found_or_error(e)),
                    };

                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(10),
                        child.wait_with_output(),
                    )
                    .await
                    {
                        Ok(Ok(o)) if o.status.success() => {
                            ToolResult::text(format!("Switched to tab {idx}"))
                        }
                        Ok(Ok(o)) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            ToolResult::error(format!("tab switch failed: {stderr}"))
                        }
                        Ok(Err(e)) => ToolResult::error(format!("Process error: {e}")),
                        Err(_) => ToolResult::error("Tab switch timed out"),
                    }
                }
                "close" => {
                    if let Some(i) = input.index {
                        // If index given, switch to that tab first then close
                        let idx_str = i.to_string();
                        let _ = command_with_session(session_ref)
                            .args(["tab", &idx_str])
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .kill_on_drop(true)
                            .spawn();
                        // Small delay for switch
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    }

                    let child = match command_with_session(session_ref)
                        .args(["tab", "close"])
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .kill_on_drop(true)
                        .spawn()
                    {
                        Ok(c) => c,
                        Err(e) => return ToolResult::error(not_found_or_error(e)),
                    };

                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(10),
                        child.wait_with_output(),
                    )
                    .await
                    {
                        Ok(Ok(o)) if o.status.success() => {
                            ToolResult::text("Tab closed".to_string())
                        }
                        Ok(Ok(o)) => {
                            let stderr = String::from_utf8_lossy(&o.stderr);
                            ToolResult::error(format!("tab close failed: {stderr}"))
                        }
                        Ok(Err(e)) => ToolResult::error(format!("Process error: {e}")),
                        Err(_) => ToolResult::error("Tab close timed out"),
                    }
                }
                other => ToolResult::error(format!(
                    "Unknown tab action: '{other}'. Use: new, list, switch, close"
                )),
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
        let tool = BrowserTabTool::new();
        let def = tool.def();
        assert_eq!(def.name, "browser_tab");
        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        assert_eq!(schema["required"], json!(["action"]));
        def.validate_input_schema().unwrap();
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = BrowserTabTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({}), &ctx).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn should_fail_with_unknown_action() {
        let tool = BrowserTabTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({"action": "destroy"}), &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Unknown tab action"));
    }

    #[tokio::test]
    async fn switch_without_index_should_fail() {
        let tool = BrowserTabTool::new();
        let ctx = test_ctx();
        let result = tool.call(json!({"action": "switch"}), &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("requires 'index'"));
    }
}
