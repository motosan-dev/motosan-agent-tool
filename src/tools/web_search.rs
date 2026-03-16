use std::future::Future;
use std::pin::Pin;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

const BRAVE_SEARCH_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const DEFAULT_MAX_RESULTS: u64 = 5;

/// A tool that performs web searches using the Brave Search API.
pub struct WebSearchTool {
    http: Client,
}

#[derive(Debug, Deserialize)]
struct WebSearchInput {
    query: String,
    #[serde(default = "default_max_results")]
    max_results: u64,
}

fn default_max_results() -> u64 {
    DEFAULT_MAX_RESULTS
}

#[derive(Debug, Serialize)]
struct SearchResult {
    title: String,
    url: String,
    description: String,
}

/// Brave Search API response structures.
#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveWebResult>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResult {
    title: String,
    url: String,
    description: Option<String>,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }

    /// Resolve the API key: context extra -> env var.
    fn resolve_api_key(&self, ctx: &ToolContext) -> Option<String> {
        ctx.get_str("brave_api_key")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("BRAVE_API_KEY").ok())
    }
}

impl Tool for WebSearchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "web_search".to_string(),
            description: "Search the web using the Brave Search API. Returns a list of \
                results with title, URL, and description."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        let ctx = ctx.clone();
        Box::pin(async move {
            let start = std::time::Instant::now();

            let input: WebSearchInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            let api_key = match self.resolve_api_key(&ctx) {
                Some(k) => k,
                None => {
                    return ToolResult::error(
                        "Brave Search API key is not configured. \
                         Set it via ctx.extra[\"brave_api_key\"] or the BRAVE_API_KEY \
                         environment variable.",
                    )
                }
            };

            let response = match self
                .http
                .get(BRAVE_SEARCH_ENDPOINT)
                .header("X-Subscription-Token", &api_key)
                .header("Accept", "application/json")
                .query(&[
                    ("q", input.query.as_str()),
                    ("count", &input.max_results.to_string()),
                ])
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    return ToolResult::error(format!("Failed to call Brave Search API: {e}"))
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return ToolResult::error(format!("Brave Search API error {status}: {body}"));
            }

            let brave_response: BraveSearchResponse = match response.json().await {
                Ok(r) => r,
                Err(e) => {
                    return ToolResult::error(format!("Failed to parse Brave Search response: {e}"))
                }
            };

            let results: Vec<SearchResult> = brave_response
                .web
                .map(|web| {
                    web.results
                        .into_iter()
                        .map(|r| SearchResult {
                            title: r.title,
                            url: r.url,
                            description: r.description.unwrap_or_default(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Format as readable text.
            let mut text = format!("Found {} results:\n\n", results.len());
            for (i, r) in results.iter().enumerate() {
                text.push_str(&format!(
                    "{}. {}\n   {}\n   {}\n\n",
                    i + 1,
                    r.title,
                    r.url,
                    r.description
                ));
            }

            let citation: String = results
                .iter()
                .map(|r| r.url.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            let duration = start.elapsed().as_millis() as u64;
            let mut result = ToolResult::text(text.trim()).with_duration(duration);
            if !citation.is_empty() {
                result = result.with_citation(citation);
            }
            result
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
        let tool = WebSearchTool::new();
        let def = tool.def();
        assert_eq!(def.name, "web_search");

        let schema = def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
        assert!(schema["properties"]["max_results"].is_object());
        assert_eq!(schema["required"], json!(["query"]));
    }

    #[tokio::test]
    async fn should_fail_without_api_key() {
        // Temporarily remove env var if set.
        let prev = std::env::var("BRAVE_API_KEY").ok();
        std::env::remove_var("BRAVE_API_KEY");

        let tool = WebSearchTool::new();
        let ctx = test_ctx();
        let input = json!({"query": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result
            .as_text()
            .unwrap()
            .contains("API key is not configured"));

        // Restore env var.
        if let Some(key) = prev {
            std::env::set_var("BRAVE_API_KEY", key);
        }
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = WebSearchTool::new();
        let ctx = test_ctx();
        let input = json!({"not_query": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }
}
