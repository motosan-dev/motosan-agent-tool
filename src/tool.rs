//! The `Tool` trait, execution context, and runtime tool output type.
//!
//! `motosan-agent-tool` 0.4 aligns with `motosan-agent-primitives`: the
//! wire-format types (`ToolCall`, `ToolResult`, `ToolAnnotations`,
//! `ContentBlock`) come from primitives, and the `Tool` trait lives here
//! (per design decisions D1=B and D10=A in the primitives implementation
//! plan).
//!
//! ## What lives where
//!
//! | Type | Crate | Role |
//! |------|-------|------|
//! | [`Tool`] (trait) | `motosan-agent-tool` | Async trait every tool implements |
//! | [`ToolContext`] | `motosan-agent-tool` | Per-call execution context (incl. cancellation token) |
//! | [`ToolOutput`] | `motosan-agent-tool` | Rich return type from [`Tool::call`] (engine-side metadata) |
//! | [`ToolDef`] | `motosan-agent-tool` | Schema / description shipped to the LLM |
//! | `ToolCall` | `motosan-agent-primitives` | Assistant-issued tool invocation (wire) |
//! | `ToolResult` | `motosan-agent-primitives` | Wire reply to a `ToolCall` |
//! | `ToolAnnotations` | `motosan-agent-primitives` | Capability metadata read by `PermissionPolicy` |
//! | `ContentBlock` | `motosan-agent-primitives` | Multimodal content payload |
//!
//! The engine (in `motosan-agent-loop`) is responsible for stamping a
//! [`ToolOutput`] with its originating `tool_use_id` and converting it into
//! a wire-format `primitives::ToolResult` at the boundary — see
//! [`ToolOutput::into_tool_result`].

use std::{collections::HashMap, ops::Deref};

use async_trait::async_trait;
use motosan_agent_primitives::{ContentBlock, ToolAnnotations, ToolResult, ToolSchema};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{Error, Result};

// ---------------------------------------------------------------------------
// Tool trait
// ---------------------------------------------------------------------------

/// A tool that can be invoked by an LLM agent.
///
/// In 0.4 the trait is `async fn` (via [`macro@async_trait`]) — the explicit
/// `Pin<Box<dyn Future>>` boilerplate from 0.3.x is gone.
///
/// # Annotations are mandatory
///
/// Every tool **must** explicitly implement [`Tool::annotations`]. There is
/// no default impl on purpose: Rust's `Default` for [`ToolAnnotations`] sets
/// `destructive = false`, which is unsafe to assume — under
/// `PermissionMode::Plan` a `destructive = false` tool is allowed to run
/// even when it touches the network. See the type-level warning on
/// [`ToolAnnotations`] for the full rationale.
///
/// When in doubt, set `destructive = true`.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Return the tool definition (name, description, input schema).
    fn def(&self) -> ToolDef;

    /// Declare capability annotations.
    ///
    /// Mandatory — no default. See trait-level doc for why.
    fn annotations(&self) -> ToolAnnotations;

    /// Execute the tool with the given arguments and context.
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput;
}

// ---------------------------------------------------------------------------
// ToolDef
// ---------------------------------------------------------------------------

/// Host-side tool definition: the LLM-facing [`ToolSchema`] plus an
/// `internal_name` used for collision detection / audit correlation
/// (NOT sent to the model).
///
/// # `name` vs `internal_name`
///
/// `name` is the **public** identifier sent to the LLM in tool-calling APIs
/// (Anthropic / OpenAI). Some providers restrict the allowed character set
/// (no dots in some places) and LLM prompt clarity favors short
/// unqualified names like `place_order`.
///
/// `internal_name` is the **host-side** identifier used by the engine for
/// collision detection across stacked harnesses, audit correlation, and any
/// other internal bookkeeping. It is free-form (dotted `finance.place_order`,
/// slashed, namespaced — whatever the host chooses) and defaults to a clone
/// of `name` when not set explicitly. See [`ToolDef::with_internal_name`].
///
/// `Deserialize` is implemented manually so that legacy JSON payloads
/// without an `internal_name` field continue to deserialize cleanly
/// (`internal_name` defaults to `name` in that case). `Serialize` is the
/// standard derive — `internal_name` is always emitted.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ToolDef {
    /// Canonical model-visible schema fields, flattened to preserve the
    /// legacy serialized shape (`name`, `description`, `input_schema`).
    #[serde(flatten)]
    pub schema: ToolSchema,
    /// Host-side identifier used for collision detection / audit
    /// correlation. Defaults to a clone of `schema.name` when constructed
    /// via [`ToolDef::new`]; override with [`ToolDef::with_internal_name`].
    ///
    /// This field is NOT sent to the LLM as part of the tool-calling
    /// protocol — only `schema.name` is. It is, however, included on the
    /// `Serialize` side so audit / snapshot consumers persist the full
    /// shape.
    pub internal_name: String,
}

impl Deref for ToolDef {
    type Target = ToolSchema;

    fn deref(&self) -> &ToolSchema {
        &self.schema
    }
}

// Helper repr used only by the manual `Deserialize` impl below — keeps
// `internal_name` as `Option<String>` on the wire so old payloads (which
// don't have the field) round-trip with `internal_name == name`.
#[derive(serde::Deserialize)]
struct ToolDefRepr {
    name: String,
    description: String,
    input_schema: Value,
    #[serde(default)]
    internal_name: Option<String>,
}

impl<'de> serde::Deserialize<'de> for ToolDef {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let repr = ToolDefRepr::deserialize(deserializer)?;
        let internal_name = repr.internal_name.unwrap_or_else(|| repr.name.clone());
        Ok(Self {
            schema: ToolSchema {
                name: repr.name,
                description: repr.description,
                input_schema: repr.input_schema,
            },
            internal_name,
        })
    }
}

/// LLM-wire-safe tool-name rule (Anthropic `^[a-zA-Z0-9_-]{1,128}$`;
/// OpenAI caps at 64 — use the conservative common denominator).
fn is_wire_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl ToolDef {
    /// Construct a new `ToolDef` with `internal_name` defaulted to
    /// `name.clone()`.
    ///
    /// Use [`ToolDef::with_internal_name`] to override the host-side
    /// identifier (e.g. for namespaced harness composition like
    /// `finance.place_order`).
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        let schema = ToolSchema::new(name, description, input_schema);
        let internal_name = schema.name.clone();
        Self {
            schema,
            internal_name,
        }
    }

    /// Override `internal_name` (builder pattern). The public `name` is
    /// unchanged.
    pub fn with_internal_name(mut self, internal_name: impl Into<String>) -> Self {
        self.internal_name = internal_name.into();
        self
    }

    /// Validate the model-visible name is LLM-wire-safe. `internal_name`
    /// is intentionally NOT constrained (it is never sent to the LLM and
    /// carries host-side namespacing like `finance.place_order`).
    pub fn validate_name(&self) -> Result<()> {
        if is_wire_safe_name(&self.schema.name) {
            Ok(())
        } else {
            Err(Error::Validation(format!(
                "tool name '{}' is not LLM-wire-safe (must match ^[A-Za-z0-9_-]{{1,64}}$); \
                 put namespacing in internal_name via ToolDef::with_internal_name",
                self.schema.name
            )))
        }
    }

    /// Full registration-time validation: name + input_schema.
    pub fn validate(&self) -> Result<()> {
        self.validate_name()?;
        self.validate_input_schema()?;
        Ok(())
    }

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
// ToolOutput
// ---------------------------------------------------------------------------

/// Rich return value from [`Tool::call`].
///
/// `ToolOutput` is the **tool author's** return type. It carries the
/// content payload (as `primitives::ContentBlock` so it matches the wire
/// format) plus engine-side metadata (citation, context-injection hint,
/// duration). The engine (`motosan-agent-loop`) stamps the originating
/// `tool_use_id` and converts to a wire-format
/// [`primitives::ToolResult`](motosan_agent_primitives::ToolResult) via
/// [`ToolOutput::into_tool_result`] before sending back to the model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolOutput {
    /// Structured content blocks returned by the tool.
    ///
    /// In 0.4 this uses the shared
    /// [`ContentBlock`] type from
    /// primitives. JSON results are wrapped in a
    /// [`ContentBlock::Json`] holding a structured value (use
    /// [`ToolOutput::json`] to construct, [`ToolOutput::as_json`] to read
    /// back).
    pub content: Vec<ContentBlock>,
    /// Whether the result represents an error.
    pub is_error: bool,
    /// Source URL for citation (e.g. web_search result, fetch_url target).
    ///
    /// Engine-side metadata: stripped when converting to
    /// `primitives::ToolResult` via [`ToolOutput::into_tool_result`].
    pub citation: Option<String>,
    /// Whether this result should be injected into the next round's context.
    ///
    /// Engine-side metadata: not part of the wire `ToolResult`.
    pub inject_to_context: bool,
    /// Execution time in milliseconds.
    ///
    /// Engine-side metadata: not part of the wire `ToolResult`.
    pub duration_ms: Option<u64>,
}

impl ToolOutput {
    /// Successful text result.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text { text: text.into() }],
            is_error: false,
            citation: None,
            inject_to_context: false,
            duration_ms: None,
        }
    }

    /// Successful JSON result.
    ///
    /// The value is wrapped in a [`ContentBlock::Json`] so downstream
    /// processors can walk the structured payload without re-parsing a
    /// string. Use [`ToolOutput::as_json`] to extract it back.
    pub fn json(value: Value) -> Self {
        Self {
            content: vec![ContentBlock::Json { value }],
            is_error: false,
            citation: None,
            inject_to_context: false,
            duration_ms: None,
        }
    }

    /// Error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::Text {
                text: message.into(),
            }],
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
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }

    /// Try to extract the first structured JSON value.
    ///
    /// Convenience helper for tools that consume another tool's
    /// [`ToolOutput::json`] result and need the typed value back.
    /// Prefers [`ContentBlock::Json`]; falls back to parsing the first
    /// [`ContentBlock::Text`] as JSON.
    pub fn as_json(&self) -> Option<Value> {
        for c in &self.content {
            if let ContentBlock::Json { value } = c {
                return Some(value.clone());
            }
        }
        let text = self.as_text()?;
        serde_json::from_str(text).ok()
    }

    /// Convert this output into a wire-format
    /// [`primitives::ToolResult`](motosan_agent_primitives::ToolResult).
    ///
    /// Strips engine-side metadata (`citation`, `inject_to_context`,
    /// `duration_ms`); the engine reads those off the original
    /// `ToolOutput` separately before discarding them.
    pub fn into_tool_result(self, tool_use_id: impl Into<String>) -> ToolResult {
        ToolResult {
            tool_use_id: tool_use_id.into(),
            content: self.content,
            is_error: self.is_error,
        }
    }
}

// ---------------------------------------------------------------------------
// ToolContext
// ---------------------------------------------------------------------------

/// Execution context passed to every tool call.
///
/// Contains common fields shared across platforms, plus an `extra` map for
/// platform-specific data (crucible: org_id, project_id; chat: group_id,
/// etc.) and a [`CancellationToken`] the engine uses to abort the call.
///
/// # Serialization caveat
///
/// [`CancellationToken`] is not serializable. The `cancellation_token`
/// field uses `#[serde(skip, default)]` so the rest of the struct still
/// round-trips for backward compatibility with persisted 0.3.x contexts.
/// A `ToolContext` recovered from JSON will have a **fresh, never-cancelled**
/// token; never rely on round-tripping cancellation state through serde.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolContext {
    /// Who is calling the tool (agent_id or user_id).
    pub caller_id: String,
    /// Platform identifier ("crucible", "line", "telegram", "discord", etc.).
    pub platform: String,
    /// Working directory for this tool call. When set, file tools resolve
    /// relative paths against this directory instead of the process cwd.
    pub cwd: Option<std::path::PathBuf>,
    /// Platform-specific key-value extensions.
    pub extra: HashMap<String, Value>,
    /// Cancellation signal. The engine cancels this token when the user
    /// aborts, the request times out, or a hook returns `Abort`.
    ///
    /// Long-running tools (shell, browser, network) should poll
    /// [`ToolContext::is_cancelled`] periodically or pass the token into
    /// any cancellable subsystem. Short / pure-compute tools may ignore it.
    ///
    /// `#[serde(skip)]` — deserialized contexts get a fresh, never-cancelled
    /// token.
    #[serde(skip, default)]
    pub cancellation_token: CancellationToken,
}

impl ToolContext {
    /// Build a new context with the required identity fields.
    pub fn new(caller_id: impl Into<String>, platform: impl Into<String>) -> Self {
        Self {
            caller_id: caller_id.into(),
            platform: platform.into(),
            cwd: None,
            extra: HashMap::new(),
            cancellation_token: CancellationToken::new(),
        }
    }

    /// Set the working directory for this call (builder pattern).
    pub fn with_cwd(mut self, cwd: impl Into<std::path::PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Insert an extra field (builder pattern).
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }

    /// Attach an engine-provided cancellation token (builder pattern).
    pub fn with_cancellation(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = token;
        self
    }

    /// `true` if the engine has cancelled this tool call.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
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
    use std::sync::Arc;

    #[derive(Debug, Deserialize)]
    struct SearchArgs {
        query: String,
        max_results: Option<u32>,
    }

    fn search_def() -> ToolDef {
        ToolDef::new(
            "web_search",
            "Search the web",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer" }
                },
                "required": ["query"]
            }),
        )
    }

    // -- ToolDef tests (unchanged behaviour from 0.3.x) --

    #[test]
    fn validate_input_schema_accepts_valid() {
        search_def().validate_input_schema().unwrap();
    }

    #[test]
    fn validate_rejects_dotted_model_name() {
        let def = ToolDef::new(
            "demo.echo",
            "d",
            serde_json::json!({ "type": "object", "properties": {} }),
        );
        assert!(def.validate().is_err());
    }

    #[test]
    fn validate_accepts_wire_safe_name_with_dotted_internal() {
        let def = ToolDef::new(
            "demo_echo",
            "d",
            serde_json::json!({ "type": "object", "properties": {} }),
        )
        .with_internal_name("demo.echo");
        def.validate().unwrap();
    }

    #[test]
    fn validate_input_schema_rejects_missing_properties() {
        let def = ToolDef::new("bad", "bad", json!({ "type": "object" }));
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
        let def = ToolDef::new(
            "t",
            "t",
            json!({
                "type": "object",
                "properties": {
                    "lang": { "type": "string", "enum": ["en", "ja", "zh"] }
                },
                "required": ["lang"]
            }),
        );
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
    fn tool_def_composes_flattened_tool_schema() {
        let def = search_def();
        assert_eq!(def.schema.name, "web_search");
        assert_eq!(def.name, "web_search");

        let value = serde_json::to_value(&def).unwrap();
        assert_eq!(value["name"], "web_search");
        assert_eq!(value["description"], "Search the web");
        assert!(value.get("schema").is_none());
    }

    #[test]
    fn serde_roundtrip_tool_def() {
        let def = search_def();
        let json = serde_json::to_string(&def).unwrap();
        let back: ToolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(def, back);
    }

    // -- ToolDef::new + internal_name (M10 D-M10-4) --

    #[test]
    fn tool_def_new_defaults_internal_name_to_name() {
        let def = ToolDef::new("foo", "desc", json!({ "type": "object", "properties": {} }));
        assert_eq!(def.name, "foo");
        assert_eq!(def.internal_name, "foo");
    }

    #[test]
    fn tool_def_with_internal_name_overrides() {
        let def = ToolDef::new(
            "place_order",
            "buy/sell",
            json!({ "type": "object", "properties": {} }),
        )
        .with_internal_name("finance.place_order");
        // Public name unchanged — that's what goes to the LLM.
        assert_eq!(def.name, "place_order");
        // Internal name carries the namespaced identifier for host-side
        // collision detection / audit correlation.
        assert_eq!(def.internal_name, "finance.place_order");
    }

    #[test]
    fn tool_def_deserialize_legacy_without_internal_name_defaults_to_name() {
        // Old payload shape (pre-0.5.0) — no `internal_name` field.
        let legacy = r#"{
            "name": "web_search",
            "description": "Search the web",
            "input_schema": { "type": "object", "properties": {} }
        }"#;
        let def: ToolDef = serde_json::from_str(legacy).unwrap();
        assert_eq!(def.name, "web_search");
        // Defaulted to a clone of `name`.
        assert_eq!(def.internal_name, "web_search");
    }

    #[test]
    fn tool_def_deserialize_with_explicit_internal_name_preserves_it() {
        let payload = r#"{
            "name": "place_order",
            "description": "buy/sell",
            "input_schema": { "type": "object", "properties": {} },
            "internal_name": "finance.place_order"
        }"#;
        let def: ToolDef = serde_json::from_str(payload).unwrap();
        assert_eq!(def.name, "place_order");
        assert_eq!(def.internal_name, "finance.place_order");
    }

    #[test]
    fn tool_def_serialize_emits_internal_name() {
        let def = ToolDef::new(
            "place_order",
            "buy/sell",
            json!({ "type": "object", "properties": {} }),
        )
        .with_internal_name("finance.place_order");
        let s = serde_json::to_string(&def).unwrap();
        assert!(
            s.contains("\"internal_name\":\"finance.place_order\""),
            "internal_name not in serialized form: {s}"
        );
    }

    // -- ToolOutput tests (replaces 0.3.x ToolResult tests) --

    #[test]
    fn tool_output_text_roundtrip() {
        let r = ToolOutput::text("hello");
        assert!(!r.is_error);
        assert_eq!(r.as_text(), Some("hello"));
        assert!(r.citation.is_none());
    }

    #[test]
    fn tool_output_error_sets_flag() {
        let r = ToolOutput::error("boom");
        assert!(r.is_error);
        assert_eq!(r.as_text(), Some("boom"));
    }

    #[test]
    fn tool_output_json_uses_json_content_block() {
        let r = ToolOutput::json(json!({ "rate": 31.5 }));
        assert!(!r.is_error);
        // JSON should round-trip through as_json()
        let parsed = r.as_json().expect("as_json");
        assert_eq!(parsed["rate"], 31.5);
        // ...and the underlying block is a structured Json variant
        match &r.content[0] {
            ContentBlock::Json { value } => {
                assert_eq!(value["rate"], 31.5);
            }
            _ => panic!("expected Json block"),
        }
    }

    #[test]
    fn tool_output_builder_chain() {
        let r = ToolOutput::text("data")
            .with_citation("https://example.com")
            .with_inject(true)
            .with_duration(42);
        assert_eq!(r.citation.as_deref(), Some("https://example.com"));
        assert!(r.inject_to_context);
        assert_eq!(r.duration_ms, Some(42));
    }

    #[test]
    fn tool_output_into_tool_result_strips_metadata() {
        let out = ToolOutput::text("hi")
            .with_citation("https://x")
            .with_duration(7);
        let wire = out.into_tool_result("call_42");
        assert_eq!(wire.tool_use_id, "call_42");
        assert!(!wire.is_error);
        assert_eq!(wire.content.len(), 1);
        match &wire.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "hi"),
            _ => panic!("expected Text"),
        }
        // Engine-side metadata not on the wire type
        // (compile-time check: ToolResult only has tool_use_id / content / is_error)
    }

    #[test]
    fn tool_output_into_tool_result_preserves_error_flag() {
        let wire = ToolOutput::error("boom").into_tool_result("call_1");
        assert!(wire.is_error);
    }

    // -- ToolContext tests --

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

    #[test]
    fn tool_context_cwd_defaults_to_none() {
        let ctx = ToolContext::new("agent-1", "crucible");
        assert!(ctx.cwd.is_none());
    }

    #[test]
    fn tool_context_with_cwd_sets_field() {
        let ctx = ToolContext::new("agent-1", "crucible").with_cwd("/tmp/work");
        assert_eq!(ctx.cwd.as_deref(), Some(std::path::Path::new("/tmp/work")));
    }

    #[test]
    fn serde_roundtrip_tool_context_with_cwd() {
        let ctx = ToolContext::new("agent-1", "crucible").with_cwd("/tmp/work");
        let json = serde_json::to_string(&ctx).unwrap();
        let back: ToolContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cwd.as_deref(), Some(std::path::Path::new("/tmp/work")));
    }

    #[test]
    fn serde_roundtrip_tool_context_without_cwd_is_backward_compatible() {
        // Existing serialized contexts without the cwd field must still deserialize.
        let json = r#"{"caller_id":"agent-1","platform":"crucible","extra":{}}"#;
        let ctx: ToolContext = serde_json::from_str(json).unwrap();
        assert!(ctx.cwd.is_none());
        // The cancellation token field is also skipped — should deserialize
        // to a fresh, never-cancelled token.
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn tool_context_cancellation_token_is_skipped_in_serde() {
        let ctx = ToolContext::new("a", "p");
        ctx.cancellation_token.cancel();
        assert!(ctx.is_cancelled());

        let s = serde_json::to_string(&ctx).unwrap();
        // The serialized form must not mention the token field.
        assert!(!s.contains("cancellation_token"));
        // Round-tripping discards the cancellation state — by design.
        let back: ToolContext = serde_json::from_str(&s).unwrap();
        assert!(!back.is_cancelled());
    }

    #[test]
    fn tool_context_cancellation_field() {
        let token = CancellationToken::new();
        let ctx = ToolContext::new("a", "p").with_cancellation(token.clone());
        assert!(!ctx.is_cancelled());
        token.cancel();
        assert!(ctx.is_cancelled());
    }

    #[tokio::test]
    async fn arc_dyn_tool_object_safe() {
        struct TinyTool;

        #[async_trait]
        impl Tool for TinyTool {
            fn def(&self) -> ToolDef {
                ToolDef::new(
                    "tiny",
                    "tiny",
                    json!({ "type": "object", "properties": {} }),
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

            async fn call(&self, _args: Value, _ctx: &ToolContext) -> ToolOutput {
                ToolOutput::text("ok")
            }
        }

        let t: Arc<dyn Tool> = Arc::new(TinyTool);
        let _def = t.def();
        let _ann = t.annotations();
        let out = t.call(json!({}), &ToolContext::new("a", "b")).await;
        assert!(!out.is_error);
    }
}
