use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Tool, ToolAnnotations, ToolContext, ToolDef, ToolOutput};

const BRAVE_SEARCH_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const TAVILY_SEARCH_ENDPOINT: &str = "https://api.tavily.com/search";
const DEFAULT_MAX_RESULTS: u64 = 5;

/// Which search provider to use at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchProvider {
    Brave,
    Tavily,
}

/// A tool that performs web searches using the Brave Search API or Tavily Search API.
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

/// Tavily Search API response structures.
#[derive(Debug, Deserialize)]
struct TavilySearchResponse {
    results: Vec<TavilySearchResult>,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResult {
    title: String,
    url: String,
    content: String,
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

    /// Resolve the Brave API key: context extra -> env var.
    fn resolve_brave_api_key(&self, ctx: &ToolContext) -> Option<String> {
        ctx.get_str("brave_api_key")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("BRAVE_API_KEY").ok())
    }

    /// Resolve the Tavily API key: context extra -> env var.
    fn resolve_tavily_api_key(&self, ctx: &ToolContext) -> Option<String> {
        ctx.get_str("tavily_api_key")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("TAVILY_API_KEY").ok())
    }

    /// Determine which provider to use based on available keys and the
    /// optional SEARCH_PROVIDER / ctx.extra["search_provider"] override.
    fn select_provider(&self, ctx: &ToolContext) -> Result<(SearchProvider, String), String> {
        let brave_key = self.resolve_brave_api_key(ctx);
        let tavily_key = self.resolve_tavily_api_key(ctx);

        // Check explicit provider preference (case-insensitive).
        let preference = ctx
            .get_str("search_provider")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("SEARCH_PROVIDER").ok());

        match preference
            .as_deref()
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            Some("brave") => brave_key
                .map(|k| (SearchProvider::Brave, k))
                .ok_or_else(|| {
                    "SEARCH_PROVIDER is set to \"brave\" but BRAVE_API_KEY is not configured"
                        .to_string()
                }),
            Some("tavily") => tavily_key
                .map(|k| (SearchProvider::Tavily, k))
                .ok_or_else(|| {
                    "SEARCH_PROVIDER is set to \"tavily\" but TAVILY_API_KEY is not configured"
                        .to_string()
                }),
            _ => {
                // Default: prefer Tavily when its key is available, else Brave.
                if let Some(k) = tavily_key {
                    Ok((SearchProvider::Tavily, k))
                } else if let Some(k) = brave_key {
                    Ok((SearchProvider::Brave, k))
                } else {
                    Err("No search API key is configured. \
                         Set TAVILY_API_KEY (or ctx.extra[\"tavily_api_key\"]) for Tavily, \
                         or BRAVE_API_KEY (or ctx.extra[\"brave_api_key\"]) for Brave Search."
                        .to_string())
                }
            }
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "web_search".to_string(),
            description: "Search the web using the Brave Search API or Tavily Search API. \
                Returns a list of results with title, URL, and description."
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

    fn annotations(&self) -> ToolAnnotations {
        ToolAnnotations {
            read_only: false,
            destructive: false,
            network_access: true,
            idempotent: false,
        }
    }

    async fn call(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let start = std::time::Instant::now();

        let input: WebSearchInput = match serde_json::from_value(args) {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(format!("Invalid input: {e}")),
        };

        let (provider, api_key) = match self.select_provider(ctx) {
            Ok(pair) => pair,
            Err(msg) => return ToolOutput::error(msg),
        };

        let results = match provider {
            SearchProvider::Brave => self.search_brave(&api_key, &input).await,
            SearchProvider::Tavily => self.search_tavily(&api_key, &input).await,
        };

        let results = match results {
            Ok(r) => r,
            Err(e) => return e,
        };

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
        let mut result = ToolOutput::text(text.trim()).with_duration(duration);
        if !citation.is_empty() {
            result = result.with_citation(citation);
        }
        result
    }
}

impl WebSearchTool {
    async fn search_brave(
        &self,
        api_key: &str,
        input: &WebSearchInput,
    ) -> Result<Vec<SearchResult>, ToolOutput> {
        let response = self
            .http
            .get(BRAVE_SEARCH_ENDPOINT)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .query(&[
                ("q", input.query.as_str()),
                ("count", &input.max_results.to_string()),
            ])
            .send()
            .await
            .map_err(|e| ToolOutput::error(format!("Failed to call Brave Search API: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ToolOutput::error(format!(
                "Brave Search API error {status}: {body}"
            )));
        }

        let brave_response: BraveSearchResponse = response.json().await.map_err(|e| {
            ToolOutput::error(format!("Failed to parse Brave Search response: {e}"))
        })?;

        Ok(brave_response
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
            .unwrap_or_default())
    }

    async fn search_tavily(
        &self,
        api_key: &str,
        input: &WebSearchInput,
    ) -> Result<Vec<SearchResult>, ToolOutput> {
        let body = json!({
            "query": input.query,
            "max_results": input.max_results,
        });

        let response = self
            .http
            .post(TAVILY_SEARCH_ENDPOINT)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ToolOutput::error(format!("Failed to call Tavily Search API: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ToolOutput::error(format!(
                "Tavily Search API error {status}: {body}"
            )));
        }

        let tavily_response: TavilySearchResponse = response.json().await.map_err(|e| {
            ToolOutput::error(format!("Failed to parse Tavily Search response: {e}"))
        })?;

        Ok(tavily_response
            .results
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                description: r.content,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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
    #[serial]
    async fn should_fail_without_api_key() {
        // Temporarily remove env vars if set.
        let prev_brave = std::env::var("BRAVE_API_KEY").ok();
        let prev_tavily = std::env::var("TAVILY_API_KEY").ok();
        let prev_provider = std::env::var("SEARCH_PROVIDER").ok();
        std::env::remove_var("BRAVE_API_KEY");
        std::env::remove_var("TAVILY_API_KEY");
        std::env::remove_var("SEARCH_PROVIDER");

        let tool = WebSearchTool::new();
        let ctx = test_ctx();
        let input = json!({"query": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result
            .as_text()
            .unwrap()
            .contains("No search API key is configured"));

        // Restore env vars.
        if let Some(key) = prev_brave {
            std::env::set_var("BRAVE_API_KEY", key);
        }
        if let Some(key) = prev_tavily {
            std::env::set_var("TAVILY_API_KEY", key);
        }
        if let Some(key) = prev_provider {
            std::env::set_var("SEARCH_PROVIDER", key);
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

    #[test]
    #[serial]
    fn should_select_tavily_when_tavily_key_present() {
        let prev_brave = std::env::var("BRAVE_API_KEY").ok();
        let prev_tavily = std::env::var("TAVILY_API_KEY").ok();
        let prev_provider = std::env::var("SEARCH_PROVIDER").ok();
        std::env::remove_var("BRAVE_API_KEY");
        std::env::set_var("TAVILY_API_KEY", "tvly-test-key");
        std::env::remove_var("SEARCH_PROVIDER");

        let tool = WebSearchTool::new();
        let ctx = test_ctx();
        let (provider, key) = tool.select_provider(&ctx).unwrap();
        assert_eq!(provider, SearchProvider::Tavily);
        assert_eq!(key, "tvly-test-key");

        // Restore env vars.
        std::env::remove_var("TAVILY_API_KEY");
        if let Some(k) = prev_brave {
            std::env::set_var("BRAVE_API_KEY", k);
        }
        if let Some(k) = prev_tavily {
            std::env::set_var("TAVILY_API_KEY", k);
        }
        if let Some(k) = prev_provider {
            std::env::set_var("SEARCH_PROVIDER", k);
        }
    }

    #[test]
    #[serial]
    fn should_select_brave_when_only_brave_key_present() {
        let prev_brave = std::env::var("BRAVE_API_KEY").ok();
        let prev_tavily = std::env::var("TAVILY_API_KEY").ok();
        let prev_provider = std::env::var("SEARCH_PROVIDER").ok();
        std::env::set_var("BRAVE_API_KEY", "brave-test-key");
        std::env::remove_var("TAVILY_API_KEY");
        std::env::remove_var("SEARCH_PROVIDER");

        let tool = WebSearchTool::new();
        let ctx = test_ctx();
        let (provider, key) = tool.select_provider(&ctx).unwrap();
        assert_eq!(provider, SearchProvider::Brave);
        assert_eq!(key, "brave-test-key");

        // Restore env vars.
        std::env::remove_var("BRAVE_API_KEY");
        if let Some(k) = prev_brave {
            std::env::set_var("BRAVE_API_KEY", k);
        }
        if let Some(k) = prev_tavily {
            std::env::set_var("TAVILY_API_KEY", k);
        }
        if let Some(k) = prev_provider {
            std::env::set_var("SEARCH_PROVIDER", k);
        }
    }

    #[test]
    #[serial]
    fn should_respect_search_provider_override() {
        let prev_brave = std::env::var("BRAVE_API_KEY").ok();
        let prev_tavily = std::env::var("TAVILY_API_KEY").ok();
        let prev_provider = std::env::var("SEARCH_PROVIDER").ok();
        std::env::set_var("BRAVE_API_KEY", "brave-test-key");
        std::env::set_var("TAVILY_API_KEY", "tvly-test-key");
        std::env::set_var("SEARCH_PROVIDER", "brave");

        let tool = WebSearchTool::new();
        let ctx = test_ctx();
        let (provider, key) = tool.select_provider(&ctx).unwrap();
        assert_eq!(provider, SearchProvider::Brave);
        assert_eq!(key, "brave-test-key");

        // Restore env vars.
        std::env::remove_var("BRAVE_API_KEY");
        std::env::remove_var("TAVILY_API_KEY");
        std::env::remove_var("SEARCH_PROVIDER");
        if let Some(k) = prev_brave {
            std::env::set_var("BRAVE_API_KEY", k);
        }
        if let Some(k) = prev_tavily {
            std::env::set_var("TAVILY_API_KEY", k);
        }
        if let Some(k) = prev_provider {
            std::env::set_var("SEARCH_PROVIDER", k);
        }
    }

    #[test]
    #[serial]
    fn should_error_when_provider_set_but_key_missing() {
        let prev_brave = std::env::var("BRAVE_API_KEY").ok();
        let prev_tavily = std::env::var("TAVILY_API_KEY").ok();
        let prev_provider = std::env::var("SEARCH_PROVIDER").ok();
        std::env::remove_var("BRAVE_API_KEY");
        std::env::remove_var("TAVILY_API_KEY");
        std::env::set_var("SEARCH_PROVIDER", "tavily");

        let tool = WebSearchTool::new();
        let ctx = test_ctx();
        let err = tool.select_provider(&ctx).unwrap_err();
        assert!(err.contains("TAVILY_API_KEY is not configured"));

        std::env::set_var("SEARCH_PROVIDER", "brave");
        let err = tool.select_provider(&ctx).unwrap_err();
        assert!(err.contains("BRAVE_API_KEY is not configured"));

        // Restore env vars.
        std::env::remove_var("SEARCH_PROVIDER");
        if let Some(k) = prev_brave {
            std::env::set_var("BRAVE_API_KEY", k);
        }
        if let Some(k) = prev_tavily {
            std::env::set_var("TAVILY_API_KEY", k);
        }
        if let Some(k) = prev_provider {
            std::env::set_var("SEARCH_PROVIDER", k);
        }
    }
}
