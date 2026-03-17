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
