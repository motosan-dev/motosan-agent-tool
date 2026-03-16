use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// Default execution timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// A tool that executes Python code via `python3 -c`.
///
/// Captures stdout and stderr, and enforces a timeout to prevent runaway
/// processes.
pub struct PythonEvalTool {
    /// Optional path to a Python virtual environment.
    venv_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PythonEvalInput {
    code: String,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct PythonEvalOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    timed_out: bool,
}

impl Default for PythonEvalTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonEvalTool {
    pub fn new() -> Self {
        Self { venv_path: None }
    }

    /// Create a `PythonEvalTool` with a specific virtual environment path.
    pub fn with_venv(venv_path: impl Into<String>) -> Self {
        Self {
            venv_path: Some(venv_path.into()),
        }
    }

    /// Resolve the Python interpreter path.
    fn python_bin(&self) -> String {
        if let Some(ref venv) = self.venv_path {
            #[cfg(target_os = "windows")]
            {
                format!("{}/Scripts/python.exe", venv)
            }
            #[cfg(not(target_os = "windows"))]
            {
                format!("{}/bin/python", venv)
            }
        } else {
            "python3".to_string()
        }
    }

    /// Check whether the configured Python interpreter is available.
    pub async fn is_available(&self) -> bool {
        let bin = self.python_bin();
        Command::new(&bin)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Tool for PythonEvalTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "python_eval".to_string(),
            description: "Execute Python code for data analysis. The code is run via \
                python3 and stdout/stderr are captured. A 30-second timeout is enforced \
                by default."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "The Python source code to execute"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional timeout in seconds (default: 30)",
                        "default": 30
                    }
                },
                "required": ["code"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let input: PythonEvalInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            let timeout_secs = input.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).min(300);
            let bin = self.python_bin();

            let child = match Command::new(&bin)
                .arg("-c")
                .arg(&input.code)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    return ToolResult::error(format!(
                        "Failed to spawn Python process ({}): {e}",
                        bin
                    ))
                }
            };

            let timeout_dur = tokio::time::Duration::from_secs(timeout_secs);
            let result = tokio::time::timeout(timeout_dur, child.wait_with_output()).await;

            match result {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let exit_code = output.status.code();

                    let eval_output = PythonEvalOutput {
                        stdout,
                        stderr,
                        exit_code,
                        timed_out: false,
                    };

                    match serde_json::to_value(eval_output) {
                        Ok(v) => ToolResult::json(v),
                        Err(e) => ToolResult::error(format!("Failed to serialize output: {e}")),
                    }
                }
                Ok(Err(e)) => ToolResult::error(format!("Python process error: {e}")),
                Err(_) => ToolResult::error(format!(
                    "Execution timed out after {} seconds",
                    timeout_secs
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
        let tool = PythonEvalTool::new();
        let def = tool.def();
        assert_eq!(def.name, "python_eval");

        let schema = def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["code"].is_object());
        assert!(schema["properties"]["timeout_secs"].is_object());
        assert_eq!(schema["required"], json!(["code"]));
    }

    #[tokio::test]
    async fn should_execute_basic_print() {
        let tool = PythonEvalTool::new();
        if !tool.is_available().await {
            eprintln!("python3 not available, skipping test");
            return;
        }

        let ctx = test_ctx();
        let input = json!({"code": "print('hello world')"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error, "Unexpected error: {:?}", result.content);
        match &result.content[0] {
            crate::ToolContent::Json(v) => {
                assert_eq!(v["stdout"].as_str().unwrap().trim(), "hello world");
                assert_eq!(v["timed_out"].as_bool().unwrap(), false);
                assert_eq!(v["exit_code"].as_i64().unwrap(), 0);
            }
            other => panic!("Expected Json content, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_capture_stderr_on_error() {
        let tool = PythonEvalTool::new();
        if !tool.is_available().await {
            eprintln!("python3 not available, skipping test");
            return;
        }

        let ctx = test_ctx();
        let input = json!({"code": "raise ValueError('boom')"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error); // process ran, just exited non-zero
        match &result.content[0] {
            crate::ToolContent::Json(v) => {
                assert!(v["stderr"].as_str().unwrap().contains("ValueError"));
                assert_ne!(v["exit_code"].as_i64().unwrap(), 0);
            }
            other => panic!("Expected Json content, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_timeout_long_running_code() {
        let tool = PythonEvalTool::new();
        if !tool.is_available().await {
            eprintln!("python3 not available, skipping test");
            return;
        }

        let ctx = test_ctx();
        let input = json!({
            "code": "import time; time.sleep(60)",
            "timeout_secs": 1
        });
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = PythonEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"not_code": "print('hi')"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[test]
    fn should_resolve_venv_python_bin() {
        let tool = PythonEvalTool::with_venv("/opt/venvs/analysis");
        let bin = tool.python_bin();
        #[cfg(not(target_os = "windows"))]
        assert_eq!(bin, "/opt/venvs/analysis/bin/python");
    }

    #[tokio::test]
    async fn should_detect_python_availability() {
        let tool = PythonEvalTool::new();
        let _available = tool.is_available().await;
    }
}
