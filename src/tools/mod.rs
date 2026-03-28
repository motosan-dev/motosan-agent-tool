//! Built-in tool implementations.
//!
//! Each tool is gated behind its own Cargo feature flag so consumers only
//! compile (and link) the tools they actually need.

#[cfg(feature = "web_search")]
pub mod web_search;

#[cfg(feature = "fetch_url")]
pub mod fetch_url;

#[cfg(feature = "read_file")]
pub mod read_file;

#[cfg(feature = "read_pdf")]
pub mod read_pdf;

#[cfg(feature = "read_spreadsheet")]
pub mod read_spreadsheet;

#[cfg(feature = "js_eval")]
pub mod js_eval;

#[cfg(feature = "python_eval")]
pub mod python_eval;

#[cfg(feature = "datetime")]
pub mod datetime;

#[cfg(feature = "currency_convert")]
pub mod currency_convert;

// Re-exports for convenience.
#[cfg(feature = "web_search")]
pub use web_search::WebSearchTool;

#[cfg(feature = "fetch_url")]
pub use fetch_url::FetchUrlTool;

#[cfg(feature = "read_file")]
pub use read_file::ReadFileTool;

#[cfg(feature = "read_pdf")]
pub use read_pdf::ReadPdfTool;

#[cfg(feature = "read_spreadsheet")]
pub use read_spreadsheet::ReadSpreadsheetTool;

#[cfg(feature = "js_eval")]
pub use js_eval::JsEvalTool;

#[cfg(feature = "python_eval")]
pub use python_eval::PythonEvalTool;

#[cfg(feature = "datetime")]
pub use datetime::DatetimeTool;

#[cfg(feature = "currency_convert")]
pub use currency_convert::CurrencyConvertTool;

#[cfg(feature = "cost_calculator")]
pub mod cost_calculator;

#[cfg(feature = "cost_calculator")]
pub use cost_calculator::CostCalculatorTool;

#[cfg(feature = "generate_pdf")]
pub mod generate_pdf;

#[cfg(feature = "generate_pdf")]
pub use generate_pdf::GeneratePdfTool;

#[cfg(feature = "browser")]
pub mod browser_act;
#[cfg(feature = "browser")]
pub mod browser_auth;
#[cfg(feature = "browser")]
pub mod browser_common;
#[cfg(feature = "browser")]
pub mod browser_navigate;
#[cfg(feature = "browser")]
pub mod browser_read;
#[cfg(feature = "browser")]
pub mod browser_screenshot;
#[cfg(feature = "browser")]
pub mod browser_snapshot;
#[cfg(feature = "browser")]
pub mod browser_tab;
#[cfg(feature = "browser")]
pub mod browser_wait;

#[cfg(feature = "browser")]
pub use browser_act::BrowserActTool;
#[cfg(feature = "browser")]
pub use browser_auth::BrowserAuthTool;
#[cfg(feature = "browser")]
pub use browser_navigate::BrowserNavigateTool;
#[cfg(feature = "browser")]
pub use browser_read::BrowserReadTool;
#[cfg(feature = "browser")]
pub use browser_screenshot::BrowserScreenshotTool;
#[cfg(feature = "browser")]
pub use browser_snapshot::BrowserSnapshotTool;
#[cfg(feature = "browser")]
pub use browser_tab::BrowserTabTool;
#[cfg(feature = "browser")]
pub use browser_wait::BrowserWaitTool;
