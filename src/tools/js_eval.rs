use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

const EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Built-in JavaScript helper functions injected into every evaluation context.
const HELPERS: &str = r#"
function csv(text) {
    var lines = text.trim().split('\n');
    if (lines.length === 0) return [];
    var headers = lines[0].split(',').map(function(h) { return h.trim(); });
    var result = [];
    for (var i = 1; i < lines.length; i++) {
        var values = lines[i].split(',').map(function(v) { return v.trim(); });
        var obj = {};
        for (var j = 0; j < headers.length; j++) {
            var val = values[j] !== undefined ? values[j] : '';
            var num = Number(val);
            obj[headers[j]] = val !== '' && !isNaN(num) ? num : val;
        }
        result.push(obj);
    }
    return result;
}

function sum(arr) {
    var total = 0;
    for (var i = 0; i < arr.length; i++) {
        total += typeof arr[i] === 'number' ? arr[i] : Number(arr[i]) || 0;
    }
    return total;
}

function avg(arr) {
    if (arr.length === 0) return 0;
    return sum(arr) / arr.length;
}

function median(arr) {
    if (arr.length === 0) return 0;
    var sorted = arr.slice().sort(function(a, b) { return a - b; });
    var mid = Math.floor(sorted.length / 2);
    if (sorted.length % 2 === 0) {
        return (sorted[mid - 1] + sorted[mid]) / 2;
    }
    return sorted[mid];
}

function stdev(arr) {
    if (arr.length === 0) return 0;
    var mean = avg(arr);
    var sqDiffs = 0;
    for (var i = 0; i < arr.length; i++) {
        var diff = arr[i] - mean;
        sqDiffs += diff * diff;
    }
    return Math.sqrt(sqDiffs / arr.length);
}

function percentile(arr, p) {
    if (arr.length === 0) return 0;
    var sorted = arr.slice().sort(function(a, b) { return a - b; });
    var idx = (p / 100) * (sorted.length - 1);
    var lower = Math.floor(idx);
    var upper = Math.ceil(idx);
    if (lower === upper) return sorted[lower];
    var weight = idx - lower;
    return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

function groupBy(arr, key) {
    var groups = {};
    for (var i = 0; i < arr.length; i++) {
        var k = String(arr[i][key]);
        if (!groups[k]) groups[k] = [];
        groups[k].push(arr[i]);
    }
    return groups;
}

function sortBy(arr, key, desc) {
    return arr.slice().sort(function(a, b) {
        var va = a[key], vb = b[key];
        if (va < vb) return desc ? 1 : -1;
        if (va > vb) return desc ? -1 : 1;
        return 0;
    });
}

function minBy(arr, key) {
    if (arr.length === 0) return undefined;
    var best = arr[0];
    for (var i = 1; i < arr.length; i++) {
        if (arr[i][key] < best[key]) best = arr[i];
    }
    return best;
}

function maxBy(arr, key) {
    if (arr.length === 0) return undefined;
    var best = arr[0];
    for (var i = 1; i < arr.length; i++) {
        if (arr[i][key] > best[key]) best = arr[i];
    }
    return best;
}
"#;

/// A tool that executes JavaScript code in a sandboxed environment using Boa Engine.
///
/// Boa is a pure-Rust JavaScript engine, so there is no filesystem or network
/// access available to the evaluated code.
pub struct JsEvalTool;

#[derive(Debug, Deserialize)]
struct JsEvalInput {
    code: String,
}

impl Default for JsEvalTool {
    fn default() -> Self {
        Self::new()
    }
}

impl JsEvalTool {
    pub fn new() -> Self {
        Self
    }
}

/// Execute JavaScript code synchronously using Boa Engine.
fn execute_js(code: &str) -> Result<String, String> {
    use boa_engine::{Context, Source};

    let mut context = Context::default();

    // Inject helper functions.
    let helpers_source = Source::from_bytes(HELPERS.as_bytes());
    context
        .eval(helpers_source)
        .map_err(|e| format!("Failed to load helpers: {e}"))?;

    // Evaluate user code.
    let user_source = Source::from_bytes(code.as_bytes());
    let result = context
        .eval(user_source)
        .map_err(|e| format!("JS evaluation error: {e}"))?;

    let json_result = result
        .to_json(&mut context)
        .map_err(|e| format!("Failed to convert result to JSON: {e}"))?;

    serde_json::to_string(&json_result).map_err(|e| format!("Failed to serialize result: {e}"))
}

impl Tool for JsEvalTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "js_eval".to_string(),
            description: "Execute JavaScript code in a sandboxed environment. \
                No filesystem or network access is available. \
                Built-in helpers: csv(), sum(), avg(), median(), stdev(), \
                percentile(), groupBy(), sortBy(), minBy(), maxBy(). \
                Returns the result of the last expression as JSON."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "The JavaScript code to execute. The result of the last expression is returned."
                    }
                },
                "required": ["code"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let input: JsEvalInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            if input.code.trim().is_empty() {
                return ToolResult::error("Code must not be empty");
            }

            let code = input.code.clone();

            let handle = tokio::task::spawn_blocking(move || execute_js(&code));

            let result = match tokio::time::timeout(EXECUTION_TIMEOUT, handle).await {
                Ok(Ok(Ok(output))) => output,
                Ok(Ok(Err(err))) => return ToolResult::error(err),
                Ok(Err(join_err)) => {
                    return ToolResult::error(format!("JS execution thread panicked: {join_err}"))
                }
                Err(_) => return ToolResult::error("JS execution timed out after 5 seconds"),
            };

            // Parse JSON string back to Value for structured output.
            match serde_json::from_str::<serde_json::Value>(&result) {
                Ok(value) => ToolResult::json(value),
                Err(_) => ToolResult::text(result),
            }
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
        let tool = JsEvalTool::new();
        let def = tool.def();
        assert_eq!(def.name, "js_eval");

        let schema = def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["code"].is_object());
        assert_eq!(schema["required"], json!(["code"]));
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"not_code": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_fail_with_empty_code() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": "  "});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn should_evaluate_basic_arithmetic() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": "2 + 3 * 4"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error, "Unexpected error: {:?}", result.as_text());
        // Result should contain 14.
        let content = &result.content[0];
        match content {
            crate::ToolContent::Json(v) => assert_eq!(*v, json!(14)),
            crate::ToolContent::Text(s) => assert!(s.contains("14"), "got: {s}"),
        }
    }

    #[tokio::test]
    async fn should_evaluate_string_expression() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": "'hello' + ' ' + 'world'"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn should_use_sum_helper() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": "sum([10, 20, 30])"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error, "Unexpected error: {:?}", result.as_text());
    }

    #[tokio::test]
    async fn should_use_avg_helper() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": "avg([10, 20, 30])"});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn should_use_csv_helper() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": r#"
            var data = csv("name,age,score\nAlice,30,95\nBob,25,87");
            data.length
        "#});
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error, "Unexpected error: {:?}", result.as_text());
    }

    #[tokio::test]
    async fn should_report_js_syntax_error() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": "var x = {"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
    }

    #[tokio::test]
    async fn should_report_js_runtime_error() {
        let tool = JsEvalTool::new();
        let ctx = test_ctx();
        let input = json!({"code": "undefinedFunction()"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
    }

    #[test]
    fn execute_js_basic_calculation() {
        let result = execute_js("1 + 2").unwrap();
        assert_eq!(result, "3");
    }

    #[test]
    fn execute_js_helpers_loaded() {
        let result = execute_js("sum([1,2,3])").unwrap();
        assert_eq!(result, "6");
    }
}
