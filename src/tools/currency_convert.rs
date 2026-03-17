use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

const PRIMARY_API: &str = "https://open.er-api.com/v6/latest";
const FALLBACK_API: &str = "https://api.exchangerate-api.com/v4/latest";
const CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

/// Cached exchange rates for a single base currency.
#[derive(Clone)]
struct CachedRates {
    rates: HashMap<String, f64>,
    fetched_at: Instant,
}

impl CachedRates {
    fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > CACHE_TTL
    }
}

/// A tool that converts between currencies using free exchange-rate APIs.
pub struct CurrencyConvertTool {
    http: Client,
    cache: Arc<Mutex<HashMap<String, CachedRates>>>,
}

#[derive(Debug, Deserialize)]
struct CurrencyConvertInput {
    from: String,
    to: String,
    #[serde(default = "default_amount")]
    amount: f64,
}

fn default_amount() -> f64 {
    1.0
}

#[derive(Debug, Serialize)]
struct ConversionResult {
    from: String,
    to: String,
    amount: f64,
    rate: f64,
    result: f64,
    source: String,
}

/// API response shape (both open.er-api.com and exchangerate-api.com share this).
#[derive(Debug, Deserialize)]
struct ExchangeRateResponse {
    rates: HashMap<String, f64>,
}

impl Default for CurrencyConvertTool {
    fn default() -> Self {
        Self::new()
    }
}

impl CurrencyConvertTool {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Resolve optional API key from context or env.
    fn resolve_api_key(&self, ctx: &ToolContext) -> Option<String> {
        ctx.get_str("exchange_rate_api_key")
            .map(|s| s.to_string())
            .or_else(|| std::env::var("EXCHANGE_RATE_API_KEY").ok())
    }

    /// Try to get cached rates for a base currency.
    pub(crate) fn get_cached(&self, base: &str) -> Option<HashMap<String, f64>> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(base)?;
        if entry.is_expired() {
            return None;
        }
        Some(entry.rates.clone())
    }

    /// Store rates in cache.
    pub(crate) fn set_cached(&self, base: &str, rates: HashMap<String, f64>) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(
                base.to_string(),
                CachedRates {
                    rates,
                    fetched_at: Instant::now(),
                },
            );
        }
    }

    /// Fetch rates from the primary API, falling back to the secondary.
    async fn fetch_rates(
        &self,
        base: &str,
        api_key: Option<&str>,
    ) -> Result<(HashMap<String, f64>, String), String> {
        // Try primary API
        let primary_url = format!("{PRIMARY_API}/{base}");
        if let Ok(rates) = self.fetch_from_url(&primary_url, api_key).await {
            return Ok((rates, "open.er-api.com".to_string()));
        }

        // Try fallback API
        let fallback_url = format!("{FALLBACK_API}/{base}");
        match self.fetch_from_url(&fallback_url, api_key).await {
            Ok(rates) => Ok((rates, "exchangerate-api.com".to_string())),
            Err(e) => Err(format!(
                "Failed to fetch rates from both APIs. Last error: {e}"
            )),
        }
    }

    async fn fetch_from_url(
        &self,
        url: &str,
        api_key: Option<&str>,
    ) -> Result<HashMap<String, f64>, String> {
        let mut req = self.http.get(url);
        if let Some(key) = api_key {
            req = req.query(&[("apikey", key)]);
        }

        let response = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {status}: {body}"));
        }

        let data: ExchangeRateResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))?;

        Ok(data.rates)
    }
}

impl Tool for CurrencyConvertTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "currency_convert".to_string(),
            description: "Convert between currencies using live exchange rates. \
                Supports batch conversion by providing comma-separated target currencies."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": {
                        "type": "string",
                        "description": "Source currency code (e.g. \"USD\", \"AUD\")"
                    },
                    "to": {
                        "type": "string",
                        "description": "Target currency code(s), comma-separated for batch (e.g. \"TWD\" or \"TWD,USD,JPY\")"
                    },
                    "amount": {
                        "type": "number",
                        "description": "Amount to convert (default: 1.0)",
                        "default": 1.0
                    }
                },
                "required": ["from", "to"]
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

            let input: CurrencyConvertInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            let base = input.from.trim().to_uppercase();
            let targets: Vec<String> = input
                .to
                .split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| !s.is_empty())
                .collect();

            if targets.is_empty() {
                return ToolResult::error("No target currency specified in 'to' field.");
            }

            let api_key = self.resolve_api_key(&ctx);

            // Try cache first
            let (rates, source) = if let Some(cached) = self.get_cached(&base) {
                (cached, "cache".to_string())
            } else {
                match self.fetch_rates(&base, api_key.as_deref()).await {
                    Ok((rates, source)) => {
                        self.set_cached(&base, rates.clone());
                        (rates, source)
                    }
                    Err(e) => return ToolResult::error(e),
                }
            };

            // Build results
            let mut results: Vec<ConversionResult> = Vec::with_capacity(targets.len());
            for target in &targets {
                let rate = match rates.get(target.as_str()) {
                    Some(&r) => r,
                    None => {
                        return ToolResult::error(format!(
                            "Unknown currency code: {target}. \
                             Could not find rate for {base} -> {target}."
                        ));
                    }
                };
                results.push(ConversionResult {
                    from: base.clone(),
                    to: target.clone(),
                    amount: input.amount,
                    rate,
                    result: (input.amount * rate * 100.0).round() / 100.0,
                    source: source.clone(),
                });
            }

            let duration = start.elapsed().as_millis() as u64;

            let output = if results.len() == 1 {
                serde_json::to_value(&results[0]).unwrap_or(json!(null))
            } else {
                serde_json::to_value(&results).unwrap_or(json!(null))
            };

            ToolResult::json(output).with_duration(duration)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext::new("test-agent", "test")
    }

    // -- Tool def tests --

    #[test]
    fn tool_def_schema_is_valid() {
        let tool = CurrencyConvertTool::new();
        let def = tool.def();
        assert_eq!(def.name, "currency_convert");
        def.validate_input_schema().unwrap();

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["from"].is_object());
        assert!(schema["properties"]["to"].is_object());
        assert!(schema["properties"]["amount"].is_object());
        assert_eq!(schema["required"], json!(["from", "to"]));
    }

    // -- Input parsing tests --

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = CurrencyConvertTool::new();
        let ctx = test_ctx();
        let input = json!({"not_from": "USD"});
        let result = tool.call(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_fail_with_empty_to() {
        let tool = CurrencyConvertTool::new();
        let ctx = test_ctx();
        let input = json!({"from": "USD", "to": "  "});
        let result = tool.call(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("No target currency"));
    }

    // -- Mock-based tests using pre-populated cache --

    fn tool_with_cached_rates() -> CurrencyConvertTool {
        let tool = CurrencyConvertTool::new();
        let mut rates = HashMap::new();
        rates.insert("TWD".to_string(), 31.5);
        rates.insert("USD".to_string(), 1.0);
        rates.insert("JPY".to_string(), 149.8);
        rates.insert("EUR".to_string(), 0.92);
        tool.set_cached("USD", rates.clone());

        // Also cache AUD rates
        let mut aud_rates = HashMap::new();
        aud_rates.insert("TWD".to_string(), 21.0);
        aud_rates.insert("USD".to_string(), 0.667);
        aud_rates.insert("JPY".to_string(), 99.9);
        tool.set_cached("AUD", aud_rates);

        tool
    }

    #[tokio::test]
    async fn single_conversion_output_format() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({"from": "USD", "to": "TWD", "amount": 100.0});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let content = &result.content[0];
        let json_val = match content {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };

        assert_eq!(json_val["from"], "USD");
        assert_eq!(json_val["to"], "TWD");
        assert_eq!(json_val["amount"], 100.0);
        assert_eq!(json_val["rate"], 31.5);
        assert_eq!(json_val["result"], 3150.0);
        assert_eq!(json_val["source"], "cache");
    }

    #[tokio::test]
    async fn batch_conversion_comma_separated() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({"from": "USD", "to": "TWD,JPY,EUR", "amount": 50.0});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };

        let arr = json_val.as_array().expect("batch should return array");
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["to"], "TWD");
        assert_eq!(arr[1]["to"], "JPY");
        assert_eq!(arr[2]["to"], "EUR");
    }

    #[tokio::test]
    async fn cache_hit_uses_cached_rate() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();

        // First call — should use cache
        let input = json!({"from": "AUD", "to": "TWD", "amount": 3600.0});
        let result = tool.call(input, &ctx).await;
        assert!(!result.is_error);
        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };
        assert_eq!(json_val["source"], "cache");
        assert_eq!(json_val["rate"], 21.0);

        // Second call — still cache
        let input2 = json!({"from": "AUD", "to": "USD", "amount": 100.0});
        let result2 = tool.call(input2, &ctx).await;
        assert!(!result2.is_error);
        let json_val2 = match &result2.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };
        assert_eq!(json_val2["source"], "cache");
        assert_eq!(json_val2["rate"], 0.667);
    }

    #[tokio::test]
    async fn unknown_currency_returns_error() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({"from": "USD", "to": "FAKE", "amount": 100.0});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Unknown currency code"));
        assert!(result.as_text().unwrap().contains("FAKE"));
    }

    #[tokio::test]
    async fn default_amount_is_one() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({"from": "USD", "to": "TWD"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };
        assert_eq!(json_val["amount"], 1.0);
        assert_eq!(json_val["result"], 31.5);
    }

    #[tokio::test]
    async fn case_insensitive_currency_codes() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({"from": "usd", "to": "twd", "amount": 10.0});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };
        assert_eq!(json_val["from"], "USD");
        assert_eq!(json_val["to"], "TWD");
    }
}
