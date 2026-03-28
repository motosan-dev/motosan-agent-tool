//! Shared utilities for browser tool modules.
//!
//! This module provides the common binary name constant and error helper
//! used by all `browser_*` tools, eliminating duplication across 7 files.

use crate::{ToolContext, ToolResult};

/// The name of the external browser automation binary.
pub const BINARY: &str = "agent-browser";

/// Produce a helpful error message when spawning `agent-browser` fails.
///
/// If the error is `NotFound`, the message includes installation instructions.
/// Otherwise it forwards the raw I/O error.
pub fn not_found_or_error(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!(
            "agent-browser not found. Please install it:\n\
             \n  cargo install agent-browser\n\
             \nor download from https://github.com/anthropics/agent-browser\n\
             \nError: {e}"
        )
    } else {
        format!("Failed to spawn agent-browser: {e}")
    }
}

/// Build the base Command for agent-browser, injecting --session-name if provided.
pub fn command_with_session(session: Option<&str>) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(BINARY);
    if let Some(s) = session {
        cmd.arg("--session-name").arg(s);
    }
    cmd
}

/// Extract the browser session name from a ToolContext (call before entering async block).
pub fn browser_session(ctx: &ToolContext) -> Option<String> {
    ctx.get_str("browser_session").map(String::from)
}

/// Validate that a URL starts with an allowed scheme (`http://`, `https://`, or `file://`).
///
/// Returns `Ok(())` if valid, or an error `ToolResult` if not.
pub fn validate_url(url: &str) -> Result<(), ToolResult> {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://") {
        Ok(())
    } else {
        Err(ToolResult::error(format!(
            "Invalid URL: '{url}'. URL must start with http://, https://, or file://"
        )))
    }
}
