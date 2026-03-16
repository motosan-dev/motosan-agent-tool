use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

const DEFAULT_MAX_ROWS: usize = 500;

/// A tool that reads Excel (.xlsx/.xls) and CSV files, returning structured
/// JSON with the first row used as column headers.
pub struct ReadSpreadsheetTool;

#[derive(Debug, Deserialize)]
struct ReadSpreadsheetInput {
    path: String,
    sheet: Option<String>,
    max_rows: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ReadSpreadsheetOutput {
    path: String,
    sheet: String,
    headers: Vec<String>,
    row_count: usize,
    truncated: bool,
    rows: Vec<serde_json::Value>,
}

impl Default for ReadSpreadsheetTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadSpreadsheetTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for ReadSpreadsheetTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "read_spreadsheet".to_string(),
            description: "Read an Excel (.xlsx/.xls) or CSV file. Uses the first row \
                as column headers and returns a JSON array of row objects. Supports \
                optional sheet selection and row limit (default: 500)."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The path to the spreadsheet file (e.g. 'data.xlsx', 'report.csv')"
                    },
                    "sheet": {
                        "type": "string",
                        "description": "The name of the sheet to read (Excel only). Defaults to the first sheet."
                    },
                    "max_rows": {
                        "type": "integer",
                        "description": "Maximum number of data rows to return (excluding header). Defaults to 500.",
                        "default": 500
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let input: ReadSpreadsheetInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            let max_rows = input.max_rows.unwrap_or(DEFAULT_MAX_ROWS);
            let path_lower = input.path.to_lowercase();

            // Read file bytes.
            let bytes = match std::fs::read(&input.path) {
                Ok(b) => b,
                Err(e) => return ToolResult::error(format!("Failed to read file: {e}")),
            };

            let result = if path_lower.ends_with(".csv") {
                parse_csv(&bytes, max_rows, &input.path)
            } else if path_lower.ends_with(".xlsx") {
                parse_xlsx(&bytes, input.sheet.as_deref(), max_rows, &input.path)
            } else if path_lower.ends_with(".xls") {
                parse_xls(&bytes, input.sheet.as_deref(), max_rows, &input.path)
            } else {
                Err("Unsupported file format. Supported formats: .xlsx, .xls, .csv".to_string())
            };

            match result {
                Ok(output) => match serde_json::to_value(output) {
                    Ok(v) => ToolResult::json(v),
                    Err(e) => ToolResult::error(format!("Failed to serialize output: {e}")),
                },
                Err(e) => ToolResult::error(e),
            }
        })
    }
}

/// Convert a calamine `Data` cell value to a `serde_json::Value`.
fn cell_to_json(cell: &calamine::Data) -> serde_json::Value {
    match cell {
        calamine::Data::Empty => serde_json::Value::Null,
        calamine::Data::String(s) => serde_json::Value::String(s.clone()),
        calamine::Data::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        calamine::Data::Int(i) => serde_json::Value::Number((*i).into()),
        calamine::Data::Bool(b) => serde_json::Value::Bool(*b),
        calamine::Data::DateTime(dt) => serde_json::Value::String(dt.to_string()),
        calamine::Data::DateTimeIso(s) => serde_json::Value::String(s.clone()),
        calamine::Data::DurationIso(s) => serde_json::Value::String(s.clone()),
        calamine::Data::Error(e) => serde_json::Value::String(format!("#ERROR: {e:?}")),
    }
}

/// Convert a calamine `Data` cell to a header string.
fn cell_to_header(cell: &calamine::Data) -> String {
    match cell {
        calamine::Data::Empty => String::new(),
        calamine::Data::String(s) => s.clone(),
        calamine::Data::Float(f) => f.to_string(),
        calamine::Data::Int(i) => i.to_string(),
        calamine::Data::Bool(b) => b.to_string(),
        calamine::Data::DateTime(dt) => dt.to_string(),
        calamine::Data::DateTimeIso(s) => s.clone(),
        calamine::Data::DurationIso(s) => s.clone(),
        calamine::Data::Error(e) => format!("#ERROR: {e:?}"),
    }
}

/// Parse rows from a calamine Range into the output structure.
fn parse_range(
    range: &calamine::Range<calamine::Data>,
    sheet_name: &str,
    max_rows: usize,
    path: &str,
) -> Result<ReadSpreadsheetOutput, String> {
    let height = range.height();
    if height == 0 {
        return Ok(ReadSpreadsheetOutput {
            path: path.to_string(),
            sheet: sheet_name.to_string(),
            headers: vec![],
            row_count: 0,
            truncated: false,
            rows: vec![],
        });
    }

    let header_row = range.rows().next().unwrap();
    let headers: Vec<String> = header_row.iter().map(cell_to_header).collect();

    let total_data_rows = height - 1;
    let take_rows = total_data_rows.min(max_rows);
    let truncated = total_data_rows > max_rows;

    let rows: Vec<serde_json::Value> = range
        .rows()
        .skip(1)
        .take(take_rows)
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (i, cell) in row.iter().enumerate() {
                let key = headers
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("column_{}", i));
                obj.insert(key, cell_to_json(cell));
            }
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(ReadSpreadsheetOutput {
        path: path.to_string(),
        sheet: sheet_name.to_string(),
        headers,
        row_count: rows.len(),
        truncated,
        rows,
    })
}

fn parse_xlsx(
    bytes: &[u8],
    sheet: Option<&str>,
    max_rows: usize,
    path: &str,
) -> Result<ReadSpreadsheetOutput, String> {
    use calamine::{open_workbook_from_rs, Reader, Xlsx};

    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> =
        open_workbook_from_rs(cursor).map_err(|e| format!("Failed to open xlsx: {e}"))?;

    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Err("Workbook contains no sheets".to_string());
    }

    let sheet_name = match sheet {
        Some(name) => {
            if !sheet_names.contains(&name.to_string()) {
                return Err(format!(
                    "Sheet '{}' not found. Available sheets: {}",
                    name,
                    sheet_names.join(", ")
                ));
            }
            name.to_string()
        }
        None => sheet_names[0].clone(),
    };

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| format!("Failed to read sheet '{}': {e}", sheet_name))?;

    parse_range(&range, &sheet_name, max_rows, path)
}

fn parse_xls(
    bytes: &[u8],
    sheet: Option<&str>,
    max_rows: usize,
    path: &str,
) -> Result<ReadSpreadsheetOutput, String> {
    use calamine::{open_workbook_from_rs, Reader, Xls};

    let cursor = Cursor::new(bytes);
    let mut workbook: Xls<_> =
        open_workbook_from_rs(cursor).map_err(|e| format!("Failed to open xls: {e}"))?;

    let sheet_names = workbook.sheet_names().to_vec();
    if sheet_names.is_empty() {
        return Err("Workbook contains no sheets".to_string());
    }

    let sheet_name = match sheet {
        Some(name) => {
            if !sheet_names.contains(&name.to_string()) {
                return Err(format!(
                    "Sheet '{}' not found. Available sheets: {}",
                    name,
                    sheet_names.join(", ")
                ));
            }
            name.to_string()
        }
        None => sheet_names[0].clone(),
    };

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| format!("Failed to read sheet '{}': {e}", sheet_name))?;

    parse_range(&range, &sheet_name, max_rows, path)
}

/// Parse CSV bytes using std::io, not calamine.
fn parse_csv(bytes: &[u8], max_rows: usize, path: &str) -> Result<ReadSpreadsheetOutput, String> {
    let mut lines = std::str::from_utf8(bytes)
        .map_err(|e| format!("Invalid UTF-8 in CSV: {e}"))?
        .lines();

    let header_line = match lines.next() {
        Some(l) => l,
        None => {
            return Ok(ReadSpreadsheetOutput {
                path: path.to_string(),
                sheet: "csv".to_string(),
                headers: vec![],
                row_count: 0,
                truncated: false,
                rows: vec![],
            })
        }
    };

    let headers: Vec<String> = header_line
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let mut rows = Vec::new();
    let mut truncated = false;

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if rows.len() >= max_rows {
            truncated = true;
            break;
        }
        let mut obj = serde_json::Map::new();
        for (i, field) in line.split(',').enumerate() {
            let field = field.trim();
            let key = headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("column_{}", i));
            let value = if field.is_empty() {
                serde_json::Value::Null
            } else if let Ok(n) = field.parse::<i64>() {
                serde_json::Value::Number(n.into())
            } else if let Ok(f) = field.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(field.to_string()))
            } else if field.eq_ignore_ascii_case("true") {
                serde_json::Value::Bool(true)
            } else if field.eq_ignore_ascii_case("false") {
                serde_json::Value::Bool(false)
            } else {
                serde_json::Value::String(field.to_string())
            };
            obj.insert(key, value);
        }
        rows.push(serde_json::Value::Object(obj));
    }

    Ok(ReadSpreadsheetOutput {
        path: path.to_string(),
        sheet: "csv".to_string(),
        headers,
        row_count: rows.len(),
        truncated,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new("test-agent", "test")
    }

    #[test]
    fn should_have_correct_name_and_schema() {
        let tool = ReadSpreadsheetTool::new();
        let def = tool.def();
        assert_eq!(def.name, "read_spreadsheet");

        let schema = def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["sheet"].is_object());
        assert!(schema["properties"]["max_rows"].is_object());
        assert_eq!(schema["required"], json!(["path"]));
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = ReadSpreadsheetTool::new();
        let ctx = test_ctx();
        let input = json!({"not_path": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[test]
    fn should_parse_csv_bytes() {
        let csv_data = b"name,age,active\nAlice,30,true\nBob,25,false\n";
        let result = parse_csv(csv_data, 500, "test.csv").unwrap();

        assert_eq!(result.path, "test.csv");
        assert_eq!(result.sheet, "csv");
        assert_eq!(result.headers, vec!["name", "age", "active"]);
        assert_eq!(result.row_count, 2);
        assert!(!result.truncated);

        assert_eq!(result.rows[0]["name"], "Alice");
        assert_eq!(result.rows[0]["age"], 30);
        assert_eq!(result.rows[0]["active"], true);
        assert_eq!(result.rows[1]["name"], "Bob");
        assert_eq!(result.rows[1]["age"], 25);
        assert_eq!(result.rows[1]["active"], false);
    }

    #[test]
    fn should_truncate_csv_at_max_rows() {
        let csv_data = b"id,val\n1,a\n2,b\n3,c\n4,d\n5,e\n";
        let result = parse_csv(csv_data, 3, "test.csv").unwrap();

        assert_eq!(result.row_count, 3);
        assert!(result.truncated);
        assert_eq!(result.rows[0]["id"], 1);
        assert_eq!(result.rows[2]["id"], 3);
    }

    #[test]
    fn should_handle_empty_csv() {
        let csv_data = b"name,age\n";
        let result = parse_csv(csv_data, 500, "empty.csv").unwrap();

        assert_eq!(result.headers, vec!["name", "age"]);
        assert_eq!(result.row_count, 0);
        assert!(!result.truncated);
    }

    #[test]
    fn should_parse_csv_with_empty_fields() {
        let csv_data = b"a,b,c\n1,,3\n,2,\n";
        let result = parse_csv(csv_data, 500, "sparse.csv").unwrap();

        assert_eq!(result.row_count, 2);
        assert_eq!(result.rows[0]["a"], 1);
        assert!(result.rows[0]["b"].is_null());
        assert_eq!(result.rows[0]["c"], 3);
        assert!(result.rows[1]["a"].is_null());
        assert_eq!(result.rows[1]["b"], 2);
        assert!(result.rows[1]["c"].is_null());
    }

    #[test]
    fn should_parse_csv_with_float_values() {
        let csv_data = b"price,qty\n19.99,1.5\n0.01,100\n";
        let result = parse_csv(csv_data, 500, "floats.csv").unwrap();

        assert_eq!(result.row_count, 2);
        assert_eq!(result.rows[0]["price"], 19.99);
        assert_eq!(result.rows[0]["qty"], 1.5);
        assert_eq!(result.rows[1]["qty"], 100);
    }

    #[test]
    fn should_fail_on_bad_xlsx_bytes() {
        let bad_bytes = b"not a real xlsx file";
        let result = parse_xlsx(bad_bytes, None, 500, "bad.xlsx");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to open xlsx"));
    }

    #[test]
    fn should_fail_on_bad_xls_bytes() {
        let bad_bytes = b"not a real xls file";
        let result = parse_xls(bad_bytes, None, 500, "bad.xls");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to open xls"));
    }
}
