use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Error, Result};

// ---------------------------------------------------------------------------
// Tool trait
// ---------------------------------------------------------------------------

/// A tool that can be called by an LLM agent.
///
/// Shared between motosan-chat (chat bots) and crucible-agent (forum pipelines).
pub trait Tool: Send + Sync {
    /// Return the tool definition (name, description, input schema).
    fn def(&self) -> ToolDef;

    /// Execute the tool with the given arguments and context.
    fn call(
        &self,
        args: Value,
        ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// ToolDef
// ---------------------------------------------------------------------------

/// Definition of a tool, suitable for serialization to LLM APIs (Claude, OpenAI, etc.).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolDef {
    /// Validate that the input_schema itself is well-formed.
    pub fn validate_input_schema(&self) -> Result<()> {
        let schema = self
            .input_schema
            .as_object()
            .ok_or_else(|| Error::Validation("input_schema must be a JSON object".into()))?;

        if schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err(Error::Validation(
                "input_schema.type must be \"object\"".into(),
            ));
        }

        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(|| Error::Validation("input_schema.properties must be an object".into()))?;

        if let Some(required) = schema.get("required") {
            let required = required.as_array().ok_or_else(|| {
                Error::Validation("input_schema.required must be an array".into())
            })?;

            for field in required {
                let field_name = field.as_str().ok_or_else(|| {
                    Error::Validation("input_schema.required entries must be strings".into())
                })?;
                if !properties.contains_key(field_name) {
                    return Err(Error::Validation(format!(
                        "required field not in properties: {field_name}"
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate arguments against the input_schema (type checking + enum checking).
    pub fn validate_args(&self, args: &Value) -> Result<()> {
        self.validate_input_schema()?;

        let schema = self
            .input_schema
            .as_object()
            .ok_or_else(|| Error::Validation("input_schema must be a JSON object".into()))?;
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(|| Error::Validation("input_schema.properties must be an object".into()))?;

        let args = args
            .as_object()
            .ok_or_else(|| Error::Validation("tool args must be a JSON object".into()))?;

        // Check required fields
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for field in required {
                let field_name = field.as_str().ok_or_else(|| {
                    Error::Validation("required field name must be string".into())
                })?;
                if !args.contains_key(field_name) {
                    return Err(Error::MissingField(field_name.into()));
                }
            }
        }

        // Type and enum checking
        for (key, value) in args {
            let Some(spec) = properties.get(key).and_then(Value::as_object) else {
                continue;
            };

            if let Some(expected_type) = spec.get("type").and_then(Value::as_str) {
                let type_matches = match expected_type {
                    "string" => value.is_string(),
                    "number" => value.is_number(),
                    "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
                    "boolean" => value.is_boolean(),
                    "object" => value.is_object(),
                    "array" => value.is_array(),
                    "null" => value.is_null(),
                    _ => true,
                };

                if !type_matches {
                    return Err(Error::Validation(format!(
                        "field {key} expected type {expected_type}"
                    )));
                }
            }

            if let Some(enum_values) = spec.get("enum").and_then(Value::as_array) {
                if !enum_values.contains(value) {
                    return Err(Error::Validation(format!("field {key} is not in enum")));
                }
            }
        }

        Ok(())
    }

    /// Validate and deserialize args into a typed struct.
    pub fn parse_args<T: DeserializeOwned>(&self, args: Value) -> Result<T> {
        self.validate_args(&args)?;
        serde_json::from_value(args).map_err(|err| Error::Parse(format!("invalid args: {err}")))
    }
}

// ---------------------------------------------------------------------------
// ToolResult
// ---------------------------------------------------------------------------

/// Result returned by tool execution.
///
/// Combines motosan-chat's typed content with crucible-agent's metadata fields.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolResult {
    /// Structured content blocks returned by the tool.
    pub content: Vec<ToolContent>,
    /// Whether the result represents an error.
    pub is_error: bool,
    /// Source URL for citation (e.g. web_search result, fetch_url target).
    pub citation: Option<String>,
    /// Whether this result should be injected into the next round's context.
    pub inject_to_context: bool,
    /// Execution time in milliseconds.
    pub duration_ms: Option<u64>,
}

impl ToolResult {
    /// Successful text result.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text(text.into())],
            is_error: false,
            citation: None,
            inject_to_context: false,
            duration_ms: None,
        }
    }

    /// Successful JSON result.
    pub fn json(value: Value) -> Self {
        Self {
            content: vec![ToolContent::Json(value)],
            is_error: false,
            citation: None,
            inject_to_context: false,
            duration_ms: None,
        }
    }

    /// Error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text(message.into())],
            is_error: true,
            citation: None,
            inject_to_context: false,
            duration_ms: None,
        }
    }

    /// Set citation on this result (builder pattern).
    pub fn with_citation(mut self, citation: impl Into<String>) -> Self {
        self.citation = Some(citation.into());
        self
    }

    /// Set inject_to_context on this result.
    pub fn with_inject(mut self, inject: bool) -> Self {
        self.inject_to_context = inject;
        self
    }

    /// Set duration_ms on this result.
    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    /// Get the first text content, if any.
    pub fn as_text(&self) -> Option<&str> {
        self.content.iter().find_map(|c| match c {
            ToolContent::Text(s) => Some(s.as_str()),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------------------
// ToolContent
// ---------------------------------------------------------------------------

/// A single content block in a tool result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ToolContent {
    #[serde(rename = "text")]
    /// Plain text content.
    Text(String),
    /// Structured JSON content.
    #[serde(rename = "json")]
    Json(Value),
}

// ---------------------------------------------------------------------------
// ToolContext
// ---------------------------------------------------------------------------

/// Execution context passed to every tool call.
///
/// Contains common fields shared across platforms, plus an `extra` map for
/// platform-specific data (crucible: org_id, project_id; chat: group_id, etc.).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolContext {
    /// Who is calling the tool (agent_id or user_id).
    pub caller_id: String,
    /// Platform identifier ("crucible", "line", "telegram", "discord", etc.).
    pub platform: String,
    /// Platform-specific key-value extensions.
    pub extra: HashMap<String, Value>,
}

impl ToolContext {
    pub fn new(caller_id: impl Into<String>, platform: impl Into<String>) -> Self {
        Self {
            caller_id: caller_id.into(),
            platform: platform.into(),
            extra: HashMap::new(),
        }
    }

    /// Insert an extra field (builder pattern).
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    /// Get a string value from extra.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.extra.get(key)?.as_str()
    }

    /// Get a u64 value from extra.
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.extra.get(key)?.as_u64()
    }

    /// Get a bool value from extra.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.extra.get(key)?.as_bool()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize)]
    struct SearchArgs {
        query: String,
        max_results: Option<u32>,
    }

    fn search_def() -> ToolDef {
        ToolDef {
            name: "web_search".into(),
            description: "Search the web".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer" }
                },
                "required": ["query"]
            }),
        }
    }

    #[test]
    fn validate_input_schema_accepts_valid() {
        search_def().validate_input_schema().unwrap();
    }

    #[test]
    fn validate_input_schema_rejects_missing_properties() {
        let def = ToolDef {
            name: "bad".into(),
            description: "bad".into(),
            input_schema: json!({ "type": "object" }),
        };
        assert!(def.validate_input_schema().is_err());
    }

    #[test]
    fn validate_args_accepts_valid() {
        let def = search_def();
        let args = json!({ "query": "rust" });
        def.validate_args(&args).unwrap();
    }

    #[test]
    fn validate_args_rejects_missing_required() {
        let def = search_def();
        let args = json!({ "max_results": 5 });
        assert!(matches!(
            def.validate_args(&args),
            Err(crate::Error::MissingField(_))
        ));
    }

    #[test]
    fn validate_args_rejects_wrong_type() {
        let def = search_def();
        let args = json!({ "query": 123 });
        assert!(def.validate_args(&args).is_err());
    }

    #[test]
    fn validate_args_checks_enum() {
        let def = ToolDef {
            name: "t".into(),
            description: "t".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lang": { "type": "string", "enum": ["en", "ja", "zh"] }
                },
                "required": ["lang"]
            }),
        };
        def.validate_args(&json!({ "lang": "en" })).unwrap();
        assert!(def.validate_args(&json!({ "lang": "fr" })).is_err());
    }

    #[test]
    fn parse_args_deserializes_typed_struct() {
        let def = search_def();
        let args = json!({ "query": "rust", "max_results": 10 });
        let parsed: SearchArgs = def.parse_args(args).unwrap();
        assert_eq!(parsed.query, "rust");
        assert_eq!(parsed.max_results, Some(10));
    }

    #[test]
    fn tool_result_text_roundtrip() {
        let r = ToolResult::text("hello");
        assert!(!r.is_error);
        assert_eq!(r.as_text(), Some("hello"));
        assert!(r.citation.is_none());
    }

    #[test]
    fn tool_result_error_sets_flag() {
        let r = ToolResult::error("boom");
        assert!(r.is_error);
    }

    #[test]
    fn tool_result_builder_chain() {
        let r = ToolResult::text("data")
            .with_citation("https://example.com")
            .with_inject(true)
            .with_duration(42);
        assert_eq!(r.citation.as_deref(), Some("https://example.com"));
        assert!(r.inject_to_context);
        assert_eq!(r.duration_ms, Some(42));
    }

    #[test]
    fn serde_roundtrip_tool_def() {
        let def = search_def();
        let json = serde_json::to_string(&def).unwrap();
        let back: ToolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(def, back);
    }

    #[test]
    fn serde_roundtrip_tool_result() {
        let r = ToolResult::text("hello")
            .with_citation("https://example.com")
            .with_duration(42);
        let json = serde_json::to_string(&r).unwrap();
        let back: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.as_text(), Some("hello"));
        assert_eq!(back.citation.as_deref(), Some("https://example.com"));
        assert_eq!(back.duration_ms, Some(42));
    }

    #[test]
    fn serde_roundtrip_tool_content() {
        let text = ToolContent::Text("hi".into());
        let json_val = ToolContent::Json(json!({"key": "value"}));

        let text_json = serde_json::to_string(&text).unwrap();
        let back: ToolContent = serde_json::from_str(&text_json).unwrap();
        assert_eq!(text, back);

        let json_json = serde_json::to_string(&json_val).unwrap();
        let back: ToolContent = serde_json::from_str(&json_json).unwrap();
        assert_eq!(json_val, back);
    }

    #[test]
    fn serde_tool_content_tagged_format() {
        let text = ToolContent::Text("hi".into());
        let serialized: Value = serde_json::to_value(&text).unwrap();
        assert_eq!(serialized["type"], "text");
        assert_eq!(serialized["data"], "hi");
    }

    #[test]
    fn serde_roundtrip_tool_context() {
        let ctx = ToolContext::new("agent-1", "crucible").with("org_id", json!("motosan"));
        let json = serde_json::to_string(&ctx).unwrap();
        let back: ToolContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.caller_id, "agent-1");
        assert_eq!(back.platform, "crucible");
        assert_eq!(back.get_str("org_id"), Some("motosan"));
    }

    #[test]
    fn tool_context_extra_helpers() {
        let ctx = ToolContext::new("agent-1", "crucible")
            .with("org_id", json!("motosan"))
            .with("budget", json!(5));
        assert_eq!(ctx.get_str("org_id"), Some("motosan"));
        assert_eq!(ctx.get_u64("budget"), Some(5));
        assert_eq!(ctx.get_str("missing"), None);
    }
}
