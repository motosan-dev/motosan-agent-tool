use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::tools::currency_convert::CurrencyConvertTool;
use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// A tool that calculates study abroad cost breakdowns with currency conversion.
pub struct CostCalculatorTool {
    converter: CurrencyConvertTool,
}

// -- Input types --

#[derive(Debug, Deserialize)]
struct CostCalculatorInput {
    items: Vec<CostItem>,
    #[serde(default = "default_target_currency")]
    target_currency: String,
}

#[derive(Debug, Deserialize)]
struct CostItem {
    category: String,
    description: String,
    amount: f64,
    currency: String,
    #[serde(default = "default_quantity")]
    quantity: f64,
    #[serde(default)]
    unit: Option<String>,
}

fn default_target_currency() -> String {
    "TWD".to_string()
}

fn default_quantity() -> f64 {
    1.0
}

// -- Output types --

#[derive(Debug, Serialize)]
struct CostCalculatorOutput {
    items: Vec<CostItemResult>,
    subtotals: HashMap<String, f64>,
    total: f64,
    target_currency: String,
    rates_used: HashMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct CostItemResult {
    category: String,
    description: String,
    amount: f64,
    currency: String,
    quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    rate: f64,
    converted: f64,
}

impl Default for CostCalculatorTool {
    fn default() -> Self {
        Self::new()
    }
}

impl CostCalculatorTool {
    pub fn new() -> Self {
        Self {
            converter: CurrencyConvertTool::new(),
        }
    }

    /// Create with a pre-configured converter (useful for testing with cached rates).
    pub fn with_converter(converter: CurrencyConvertTool) -> Self {
        Self { converter }
    }

    /// Get the exchange rate from `from` to `to`.
    /// Returns 1.0 if `from == to` (no conversion needed).
    async fn get_rate(
        &self,
        from: &str,
        to: &str,
        ctx: &ToolContext,
    ) -> Result<f64, String> {
        if from == to {
            return Ok(1.0);
        }

        // Use the converter's call method to get the rate
        let args = json!({ "from": from, "to": to, "amount": 1.0 });
        let result = self.converter.call(args, ctx).await;

        if result.is_error {
            let msg = result.as_text().unwrap_or("Unknown conversion error");
            return Err(format!("Failed to convert {from} -> {to}: {msg}"));
        }

        // Extract rate from the converter result
        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => return Err("Unexpected converter response format".to_string()),
        };

        json_val["rate"]
            .as_f64()
            .ok_or_else(|| "Missing rate in converter response".to_string())
    }
}

impl Tool for CostCalculatorTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "cost_calculator".to_string(),
            description: "Calculate study abroad cost breakdowns with automatic currency \
                conversion. Groups costs by category and provides subtotals and a grand total \
                in the target currency."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "List of cost items to calculate",
                        "items": {
                            "type": "object",
                            "properties": {
                                "category": {
                                    "type": "string",
                                    "description": "Cost category (e.g. \"tuition\", \"housing\", \"food\", \"transport\")"
                                },
                                "description": {
                                    "type": "string",
                                    "description": "Description of the cost item"
                                },
                                "amount": {
                                    "type": "number",
                                    "description": "Cost amount per unit"
                                },
                                "currency": {
                                    "type": "string",
                                    "description": "Currency code (e.g. \"USD\", \"AUD\", \"GBP\")"
                                },
                                "quantity": {
                                    "type": "number",
                                    "description": "Number of units (default: 1)",
                                    "default": 1
                                },
                                "unit": {
                                    "type": "string",
                                    "description": "Unit label (e.g. \"month\", \"semester\", \"year\")"
                                }
                            },
                            "required": ["category", "description", "amount", "currency"]
                        }
                    },
                    "target_currency": {
                        "type": "string",
                        "description": "Target currency for totals (default: \"TWD\")",
                        "default": "TWD"
                    }
                },
                "required": ["items"]
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

            let input: CostCalculatorInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            if input.items.is_empty() {
                return ToolResult::error("Items list must not be empty.");
            }

            let target = input.target_currency.trim().to_uppercase();

            // Collect unique source currencies that need conversion
            let mut rates_used: HashMap<String, f64> = HashMap::new();
            let mut item_results: Vec<CostItemResult> = Vec::with_capacity(input.items.len());
            let mut subtotals: HashMap<String, f64> = HashMap::new();

            for item in &input.items {
                let from = item.currency.trim().to_uppercase();

                // Get or fetch rate
                let rate = if let Some(&cached_rate) = rates_used.get(&from) {
                    cached_rate
                } else {
                    match self.get_rate(&from, &target, &ctx).await {
                        Ok(r) => {
                            rates_used.insert(from.clone(), r);
                            r
                        }
                        Err(e) => return ToolResult::error(e),
                    }
                };

                let quantity = if item.quantity == 0.0 {
                    1.0
                } else {
                    item.quantity
                };
                let converted = (item.amount * rate * quantity * 100.0).round() / 100.0;

                // Accumulate subtotal
                *subtotals.entry(item.category.clone()).or_insert(0.0) += converted;

                item_results.push(CostItemResult {
                    category: item.category.clone(),
                    description: item.description.clone(),
                    amount: item.amount,
                    currency: from,
                    quantity,
                    unit: item.unit.clone(),
                    rate,
                    converted,
                });
            }

            // Round subtotals
            for val in subtotals.values_mut() {
                *val = (*val * 100.0).round() / 100.0;
            }

            let total: f64 = subtotals.values().sum();
            let total = (total * 100.0).round() / 100.0;

            let output = CostCalculatorOutput {
                items: item_results,
                subtotals,
                total,
                target_currency: target,
                rates_used,
            };

            let duration = start.elapsed().as_millis() as u64;

            match serde_json::to_value(&output) {
                Ok(v) => ToolResult::json(v).with_duration(duration),
                Err(e) => ToolResult::error(format!("Failed to serialize output: {e}")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext::new("test-agent", "test")
    }

    /// Build a CostCalculatorTool with pre-populated exchange rates in cache.
    fn tool_with_cached_rates() -> CostCalculatorTool {
        let converter = CurrencyConvertTool::new();

        let mut usd_rates = HashMap::new();
        usd_rates.insert("TWD".to_string(), 31.5);
        usd_rates.insert("USD".to_string(), 1.0);
        usd_rates.insert("JPY".to_string(), 149.8);
        usd_rates.insert("EUR".to_string(), 0.92);
        converter.set_cached("USD", usd_rates);

        let mut aud_rates = HashMap::new();
        aud_rates.insert("TWD".to_string(), 21.0);
        aud_rates.insert("USD".to_string(), 0.667);
        aud_rates.insert("AUD".to_string(), 1.0);
        converter.set_cached("AUD", aud_rates);

        let mut twd_rates = HashMap::new();
        twd_rates.insert("TWD".to_string(), 1.0);
        twd_rates.insert("USD".to_string(), 0.0317);
        converter.set_cached("TWD", twd_rates);

        CostCalculatorTool::with_converter(converter)
    }

    // -- Tool def tests --

    #[test]
    fn tool_def_schema_is_valid() {
        let tool = CostCalculatorTool::new();
        let def = tool.def();
        assert_eq!(def.name, "cost_calculator");
        def.validate_input_schema().unwrap();

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["items"].is_object());
        assert!(schema["properties"]["target_currency"].is_object());
        assert_eq!(schema["required"], json!(["items"]));
    }

    // -- Same currency (no conversion) --

    #[tokio::test]
    async fn same_currency_items_no_conversion() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({
            "items": [
                {"category": "tuition", "description": "Semester fee", "amount": 50000.0, "currency": "TWD"},
                {"category": "housing", "description": "Dorm per month", "amount": 8000.0, "currency": "TWD", "quantity": 6, "unit": "month"}
            ],
            "target_currency": "TWD"
        });

        let result = tool.call(input, &ctx).await;
        assert!(!result.is_error, "Expected success but got: {:?}", result.as_text());

        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };

        // All items should have rate = 1.0
        let items = json_val["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["rate"], 1.0);
        assert_eq!(items[1]["rate"], 1.0);

        // Tuition: 50000 * 1 * 1 = 50000
        assert_eq!(items[0]["converted"], 50000.0);
        // Housing: 8000 * 1 * 6 = 48000
        assert_eq!(items[1]["converted"], 48000.0);

        // Subtotals
        let subtotals = json_val["subtotals"].as_object().unwrap();
        assert_eq!(subtotals["tuition"], 50000.0);
        assert_eq!(subtotals["housing"], 48000.0);

        // Total
        assert_eq!(json_val["total"], 98000.0);
        assert_eq!(json_val["target_currency"], "TWD");
    }

    // -- Mixed currencies --

    #[tokio::test]
    async fn mixed_currencies_with_conversion() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({
            "items": [
                {"category": "tuition", "description": "University tuition", "amount": 15000.0, "currency": "USD", "quantity": 1, "unit": "year"},
                {"category": "housing", "description": "Rent", "amount": 1200.0, "currency": "AUD", "quantity": 12, "unit": "month"},
                {"category": "food", "description": "Monthly groceries", "amount": 5000.0, "currency": "TWD", "quantity": 12, "unit": "month"}
            ],
            "target_currency": "TWD"
        });

        let result = tool.call(input, &ctx).await;
        assert!(!result.is_error, "Expected success but got: {:?}", result.as_text());

        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };

        let items = json_val["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);

        // Tuition: 15000 * 31.5 * 1 = 472500
        assert_eq!(items[0]["rate"], 31.5);
        assert_eq!(items[0]["converted"], 472500.0);

        // Housing: 1200 * 21.0 * 12 = 302400
        assert_eq!(items[1]["rate"], 21.0);
        assert_eq!(items[1]["converted"], 302400.0);

        // Food: 5000 * 1.0 * 12 = 60000
        assert_eq!(items[2]["rate"], 1.0);
        assert_eq!(items[2]["converted"], 60000.0);

        // Subtotals
        let subtotals = json_val["subtotals"].as_object().unwrap();
        assert_eq!(subtotals["tuition"], 472500.0);
        assert_eq!(subtotals["housing"], 302400.0);
        assert_eq!(subtotals["food"], 60000.0);

        // Total = 472500 + 302400 + 60000 = 834900
        assert_eq!(json_val["total"], 834900.0);
    }

    // -- Category subtotals aggregation --

    #[tokio::test]
    async fn category_subtotals_aggregate_correctly() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({
            "items": [
                {"category": "housing", "description": "Rent", "amount": 1000.0, "currency": "USD"},
                {"category": "housing", "description": "Utilities", "amount": 200.0, "currency": "USD"},
                {"category": "food", "description": "Groceries", "amount": 300.0, "currency": "USD"}
            ],
            "target_currency": "TWD"
        });

        let result = tool.call(input, &ctx).await;
        assert!(!result.is_error);

        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };

        let subtotals = json_val["subtotals"].as_object().unwrap();
        // Housing: (1000 + 200) * 31.5 = 37800
        assert_eq!(subtotals["housing"], 37800.0);
        // Food: 300 * 31.5 = 9450
        assert_eq!(subtotals["food"], 9450.0);

        // Total = 37800 + 9450 = 47250
        assert_eq!(json_val["total"], 47250.0);
    }

    // -- Total is sum of all converted amounts --

    #[tokio::test]
    async fn total_is_sum_of_all_converted() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({
            "items": [
                {"category": "a", "description": "Item 1", "amount": 100.0, "currency": "USD"},
                {"category": "b", "description": "Item 2", "amount": 200.0, "currency": "AUD"},
                {"category": "c", "description": "Item 3", "amount": 1000.0, "currency": "TWD"}
            ],
            "target_currency": "TWD"
        });

        let result = tool.call(input, &ctx).await;
        assert!(!result.is_error);

        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };

        let items = json_val["items"].as_array().unwrap();
        let sum: f64 = items.iter().map(|i| i["converted"].as_f64().unwrap()).sum();
        let sum = (sum * 100.0).round() / 100.0;
        assert_eq!(json_val["total"].as_f64().unwrap(), sum);
    }

    // -- Error cases --

    #[tokio::test]
    async fn invalid_input_returns_error() {
        let tool = CostCalculatorTool::new();
        let ctx = test_ctx();
        let input = json!({"not_items": []});
        let result = tool.call(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn empty_items_returns_error() {
        let tool = CostCalculatorTool::new();
        let ctx = test_ctx();
        let input = json!({"items": []});
        let result = tool.call(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn default_target_currency_is_twd() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({
            "items": [
                {"category": "misc", "description": "Book", "amount": 50.0, "currency": "USD"}
            ]
        });

        let result = tool.call(input, &ctx).await;
        assert!(!result.is_error);

        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };
        assert_eq!(json_val["target_currency"], "TWD");
    }

    #[tokio::test]
    async fn quantity_defaults_to_one() {
        let tool = tool_with_cached_rates();
        let ctx = test_ctx();
        let input = json!({
            "items": [
                {"category": "tuition", "description": "Fee", "amount": 1000.0, "currency": "USD"}
            ],
            "target_currency": "TWD"
        });

        let result = tool.call(input, &ctx).await;
        assert!(!result.is_error);

        let json_val = match &result.content[0] {
            crate::ToolContent::Json(v) => v,
            _ => panic!("Expected JSON content"),
        };

        let items = json_val["items"].as_array().unwrap();
        assert_eq!(items[0]["quantity"], 1.0);
        // 1000 * 31.5 * 1 = 31500
        assert_eq!(items[0]["converted"], 31500.0);
    }
}
