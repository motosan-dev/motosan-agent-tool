//! `motosan-agent-tool` — the `Tool` trait, runtime tool execution context,
//! and a registry, plus a catalogue of built-in tools.
//!
//! In 0.4 this crate is wired to `motosan-agent-primitives` for the
//! wire-format types (`ToolCall`, `ToolResult`, `ToolAnnotations`,
//! `ContentBlock`). See [`tool`] for the layering / migration story.

pub mod error;
pub mod registry;
pub mod tool;
pub mod tools;

pub use error::{Error, Result};
pub use registry::ToolRegistry;
pub use serde_json::Value;
pub use tool::{Tool, ToolContext, ToolDef, ToolOutput};

// Convenience re-exports of the wire-format types from primitives so
// downstream callers can `use motosan_agent_tool::*` without separately
// depending on `motosan-agent-primitives` for these types.
pub use motosan_agent_primitives::{ContentBlock, ToolAnnotations, ToolCall, ToolResult};
