use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::tool::{Tool, ToolDef};

/// Thread-safe registry that holds named tools.
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.def().name.clone();
        self.tools.write().await.insert(name, tool);
    }

    /// Get a tool by name.
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().await.get(name).cloned()
    }

    /// List all tool definitions (sorted by name for determinism).
    pub async fn list_defs(&self) -> Vec<ToolDef> {
        let tools = self.tools.read().await;
        let mut defs: Vec<ToolDef> = tools.values().map(|t| t.def()).collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Number of registered tools.
    pub async fn len(&self) -> usize {
        self.tools.read().await.len()
    }

    /// Whether the registry is empty.
    pub async fn is_empty(&self) -> bool {
        self.tools.read().await.is_empty()
    }

    /// Remove a tool by name. Returns the removed tool, if any.
    pub async fn deregister(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.write().await.remove(name)
    }

    /// Remove all tools from the registry.
    pub async fn clear(&self) {
        self.tools.write().await.clear();
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{ToolContext, ToolResult};
    use serde_json::json;
    use std::future::Future;
    use std::pin::Pin;

    struct EchoTool;

    impl Tool for EchoTool {
        fn def(&self) -> ToolDef {
            ToolDef {
                name: "echo".into(),
                description: "Echo back the input".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" }
                    },
                    "required": ["text"]
                }),
            }
        }

        fn call(
            &self,
            args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
            Box::pin(async move {
                let text = args["text"].as_str().unwrap_or("").to_string();
                ToolResult::text(text)
            })
        }
    }

    #[tokio::test]
    async fn register_and_get() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;

        assert_eq!(registry.len().await, 1);
        let tool = registry.get("echo").await.unwrap();
        assert_eq!(tool.def().name, "echo");
    }

    #[tokio::test]
    async fn list_defs_sorted() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;

        let defs = registry.list_defs().await;
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo");
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let registry = ToolRegistry::new();
        assert!(registry.get("missing").await.is_none());
    }

    #[tokio::test]
    async fn deregister_removes_tool() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;
        assert_eq!(registry.len().await, 1);

        let removed = registry.deregister("echo").await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().def().name, "echo");
        assert_eq!(registry.len().await, 0);
    }

    #[tokio::test]
    async fn deregister_missing_returns_none() {
        let registry = ToolRegistry::new();
        assert!(registry.deregister("missing").await.is_none());
    }

    #[tokio::test]
    async fn clear_removes_all() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).await;
        assert!(!registry.is_empty().await);

        registry.clear().await;
        assert!(registry.is_empty().await);
    }
}
