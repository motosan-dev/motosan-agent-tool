use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::{Tool, ToolAnnotations, ToolContext, ToolDef, ToolOutput};

/// A tool that reads local files from disk.
///
/// Includes path traversal protection (rejects paths containing `..`) and
/// validates that files are valid UTF-8.
pub struct ReadFileTool;

#[derive(Debug, Deserialize)]
struct ReadFileInput {
    path: String,
    max_chars: Option<usize>,
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn def(&self) -> ToolDef {
        ToolDef::new(
            "read_file".to_string(),
            "Read the contents of a local file. Returns the text content \
                of the file. Only UTF-8 files are supported."
                .to_string(),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the file to read"
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Maximum number of characters to return (optional)"
                    }
                },
                "required": ["path"]
            }),
        )
    }

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            read_only: true,
            destructive: false,
            network_access: false,
            idempotent: true,
        }
    }

    async fn call(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let cwd = ctx.cwd.clone();

        let input: ReadFileInput = match serde_json::from_value(args) {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(format!("Invalid input: {e}")),
        };

        // Path traversal protection.
        if input.path.contains("..") {
            return ToolOutput::error(
                "Path traversal detected: paths containing '..' are not allowed",
            );
        }

        let resolved = if std::path::Path::new(&input.path).is_absolute() {
            std::path::PathBuf::from(&input.path)
        } else if let Some(base) = &cwd {
            base.join(&input.path)
        } else {
            std::path::PathBuf::from(&input.path)
        };
        let path = resolved.as_path();
        if !path.exists() {
            return ToolOutput::error(format!("File not found: {}", input.path));
        }
        if !path.is_file() {
            return ToolOutput::error(format!("Not a file: {}", input.path));
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                // Check if it's likely a binary file (invalid UTF-8).
                if e.kind() == std::io::ErrorKind::InvalidData {
                    return ToolOutput::error(
                        "File appears to be binary. Only UTF-8 text files are supported.",
                    );
                }
                return ToolOutput::error(format!("Failed to read file: {e}"));
            }
        };

        let content = if let Some(max) = input.max_chars {
            if content.len() > max {
                let safe_boundary = content
                    .char_indices()
                    .map(|(idx, _)| idx)
                    .take_while(|&idx| idx <= max)
                    .last()
                    .unwrap_or(0);
                format!(
                    "{}\n\n[... truncated at {} chars, total {} chars]",
                    &content[..safe_boundary],
                    max,
                    content.len()
                )
            } else {
                content
            }
        } else {
            content
        };

        ToolOutput::text(content)
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
        let tool = ReadFileTool::new();
        let def = tool.def();
        assert_eq!(def.name, "read_file");

        let schema = def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["max_chars"].is_object());
        assert_eq!(schema["required"], json!(["path"]));
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = ReadFileTool::new();
        let ctx = test_ctx();
        let input = json!({"not_path": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_reject_path_traversal() {
        let tool = ReadFileTool::new();
        let ctx = test_ctx();
        let input = json!({"path": "/tmp/../etc/passwd"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Path traversal"));
    }

    #[tokio::test]
    async fn should_fail_for_missing_file() {
        let tool = ReadFileTool::new();
        let ctx = test_ctx();
        let input = json!({"path": "/nonexistent/file.txt"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("File not found"));
    }

    #[tokio::test]
    async fn should_read_file_contents() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("create temp dir");
        let file_path = dir.path().join("test.txt");
        let mut file = std::fs::File::create(&file_path).expect("create file");
        write!(file, "Hello from test file").expect("write file");

        let tool = ReadFileTool::new();
        let ctx = test_ctx();
        let input = json!({"path": file_path.to_str().unwrap()});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        assert_eq!(result.as_text().unwrap(), "Hello from test file");
    }

    #[tokio::test]
    async fn should_resolve_relative_path_via_ctx_cwd() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("create temp dir");
        let file_path = dir.path().join("relative.txt");
        let mut file = std::fs::File::create(&file_path).expect("create file");
        write!(file, "relative content").expect("write file");

        let tool = ReadFileTool::new();
        let ctx = ToolContext::new("test-agent", "test").with_cwd(dir.path());
        let input = json!({"path": "relative.txt"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        assert_eq!(result.as_text().unwrap(), "relative content");
    }

    #[tokio::test]
    async fn should_truncate_with_max_chars() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("create temp dir");
        let file_path = dir.path().join("long.txt");
        let mut file = std::fs::File::create(&file_path).expect("create file");
        write!(file, "{}", "a".repeat(1000)).expect("write file");

        let tool = ReadFileTool::new();
        let ctx = test_ctx();
        let input = json!({"path": file_path.to_str().unwrap(), "max_chars": 100});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let text = result.as_text().unwrap();
        assert!(text.contains("truncated"));
        assert!(text.len() < 1000);
    }
}
