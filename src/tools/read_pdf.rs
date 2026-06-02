use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::{Tool, ToolAnnotations, ToolContext, ToolDef, ToolOutput};

const DEFAULT_MAX_CHARS: usize = 50_000;
const MAX_PDF_BYTES: usize = 50 * 1_048_576; // 50 MB

/// A tool that extracts text content from PDF files.
///
/// Supports both local file paths and HTTP/HTTPS URLs. When the extracted text
/// exceeds `max_chars` (default 50 000), the output is truncated.
pub struct ReadPdfTool;

#[derive(Debug, Deserialize)]
struct ReadPdfInput {
    source: String,
    #[serde(default = "default_max_chars")]
    max_chars: usize,
}

fn default_max_chars() -> usize {
    DEFAULT_MAX_CHARS
}

/// Check whether an IP address is in a private or reserved range.
fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

impl Default for ReadPdfTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadPdfTool {
    pub fn new() -> Self {
        Self
    }

    /// Read PDF bytes from a local file path.
    fn read_local_pdf(path: &str) -> Result<Vec<u8>, String> {
        let path = std::path::Path::new(path);
        if !path.exists() {
            return Err(format!("File not found: {}", path.display()));
        }
        let metadata =
            std::fs::metadata(path).map_err(|e| format!("Failed to read file metadata: {e}"))?;
        if metadata.len() as usize > MAX_PDF_BYTES {
            return Err(format!(
                "PDF too large: {} bytes exceeds {} byte limit",
                metadata.len(),
                MAX_PDF_BYTES
            ));
        }
        std::fs::read(path).map_err(|e| format!("Failed to read file: {e}"))
    }

    /// Download PDF bytes from a URL with SSRF protection.
    #[cfg(feature = "fetch_url")]
    async fn download_pdf(url: &str) -> Result<Vec<u8>, String> {
        use std::net::ToSocketAddrs;
        use std::time::Duration;

        let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| "URL has no host".to_string())?
            .to_string();
        let port = parsed.port_or_known_default().unwrap_or(80);
        let addr_str = format!("{host}:{port}");
        let addrs: Vec<_> = addr_str
            .to_socket_addrs()
            .map_err(|e| format!("Failed to resolve host: {e}"))?
            .collect();
        if addrs.is_empty() {
            return Err("Could not resolve host".to_string());
        }
        for addr in &addrs {
            if is_private_ip(addr.ip()) {
                return Err(format!(
                    "Blocked: {} resolves to private/reserved IP {}",
                    host,
                    addr.ip()
                ));
            }
        }

        let pinned_addr = addrs[0];
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .resolve(&host, pinned_addr)
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let response = client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (compatible; MotosanAgent/1.0)")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch URL: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "HTTP error {} when fetching {}",
                response.status(),
                url
            ));
        }

        if let Some(cl) = response.content_length() {
            if cl as usize > MAX_PDF_BYTES {
                return Err(format!(
                    "PDF too large: {cl} bytes exceeds {} byte limit",
                    MAX_PDF_BYTES
                ));
            }
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {e}"))?;
        if bytes.len() > MAX_PDF_BYTES {
            return Err(format!(
                "PDF too large: {} bytes exceeds {} byte limit",
                bytes.len(),
                MAX_PDF_BYTES
            ));
        }

        Ok(bytes.to_vec())
    }

    /// Without the fetch_url feature, URL downloads are not supported.
    #[cfg(not(feature = "fetch_url"))]
    async fn download_pdf(_url: &str) -> Result<Vec<u8>, String> {
        Err("URL downloads require the 'fetch_url' feature to be enabled".to_string())
    }
}

/// Extract text content from raw PDF bytes.
fn extract_text_from_pdf(bytes: &[u8]) -> Result<(String, usize), String> {
    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| format!("PDF extraction error: {e}"))?;

    let ff_count = text.matches('\u{000C}').count();
    let num_pages = if ff_count > 0 {
        ff_count + 1
    } else {
        let type_count = bytes.windows(7).filter(|w| w == b"/Type /").count();
        if type_count > 0 {
            type_count
        } else {
            1
        }
    };

    Ok((text, num_pages))
}

/// Truncate text to at most `max_chars` characters, respecting UTF-8 boundaries.
fn truncate_text(text: &str, max_chars: usize) -> (String, bool) {
    if text.len() <= max_chars {
        return (text.to_string(), false);
    }
    let safe_boundary = text
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|&idx| idx <= max_chars)
        .last()
        .unwrap_or(0);
    let boundary = text[..safe_boundary].rfind(' ').unwrap_or(safe_boundary);
    (
        format!(
            "{}\n\n[... truncated at {max_chars} chars]",
            &text[..boundary]
        ),
        true,
    )
}

#[async_trait]
impl Tool for ReadPdfTool {
    fn def(&self) -> ToolDef {
        ToolDef::new(
            "read_pdf".to_string(),
            "Extract text content from a PDF file. Accepts a local file path \
                or an HTTP/HTTPS URL. Returns the extracted text, truncated to max_chars \
                (default 50000) if the document is large."
                .to_string(),
            json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Local file path or HTTP/HTTPS URL of the PDF to read"
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Maximum number of characters to return (default: 50000)",
                        "default": 50000
                    }
                },
                "required": ["source"]
            }),
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

    async fn call(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let cwd = ctx.cwd.clone();

        let mut input: ReadPdfInput = match serde_json::from_value(args) {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(format!("Invalid input: {e}")),
        };

        if input.max_chars == 0 {
            input.max_chars = DEFAULT_MAX_CHARS;
        }

        let is_url = input.source.starts_with("http://") || input.source.starts_with("https://");

        let bytes = if is_url {
            match Self::download_pdf(&input.source).await {
                Ok(b) => b,
                Err(e) => return ToolOutput::error(e),
            }
        } else {
            let resolved = if std::path::Path::new(&input.source).is_absolute() {
                input.source.clone()
            } else if let Some(base) = &cwd {
                base.join(&input.source).to_string_lossy().into_owned()
            } else {
                input.source.clone()
            };
            match Self::read_local_pdf(&resolved) {
                Ok(b) => b,
                Err(e) => return ToolOutput::error(e),
            }
        };

        let (text, num_pages) = match extract_text_from_pdf(&bytes) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(e),
        };

        let (content, truncated) = truncate_text(&text, input.max_chars);

        let summary = format!(
            "PDF: {} pages, {} chars{}\n\n{}",
            num_pages,
            text.len(),
            if truncated { " (truncated)" } else { "" },
            content
        );

        let mut result = ToolOutput::text(summary);
        if is_url {
            result = result.with_citation(input.source);
        }
        result
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
        let tool = ReadPdfTool::new();
        let def = tool.def();
        assert_eq!(def.name, "read_pdf");

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["source"].is_object());
        assert!(schema["properties"]["max_chars"].is_object());
        assert_eq!(schema["required"], json!(["source"]));
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = ReadPdfTool::new();
        let ctx = test_ctx();
        let input = json!({"not_source": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_fail_for_missing_file() {
        let tool = ReadPdfTool::new();
        let ctx = test_ctx();
        let input = json!({"source": "/nonexistent/path/to/file.pdf"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("File not found"));
    }

    #[tokio::test]
    async fn should_resolve_relative_path_via_ctx_cwd() {
        // Copy a minimal valid PDF into a temp dir, then reference it by filename only.
        let dir = tempfile::tempdir().expect("create temp dir");
        // Use a tiny hand-crafted valid PDF (1 page, empty body).
        let minimal_pdf: &[u8] = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
            2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
            3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\n\
            xref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n\
            0000000058 00000 n \n0000000115 00000 n \n\
            trailer<</Size 4/Root 1 0 R>>\nstartxref\n190\n%%EOF";
        let pdf_name = "sample.pdf";
        std::fs::write(dir.path().join(pdf_name), minimal_pdf).expect("write pdf");

        let tool = ReadPdfTool::new();
        let ctx = ToolContext::new("test-agent", "test").with_cwd(dir.path());
        let input = json!({"source": pdf_name});
        let result = tool.call(input, &ctx).await;

        // The PDF may fail to parse (minimal PDF), but it must NOT fail with "File not found".
        if result.is_error {
            assert!(
                !result.as_text().unwrap().contains("File not found"),
                "Should have found the file via cwd, got: {}",
                result.as_text().unwrap()
            );
        }
    }

    #[test]
    fn should_truncate_long_text() {
        let text = "a".repeat(60_000);
        let (truncated, was_truncated) = truncate_text(&text, 50_000);
        assert!(was_truncated);
        assert!(truncated.len() < 60_000);
        assert!(truncated.contains("[... truncated at 50000 chars]"));
    }

    #[test]
    fn should_not_truncate_short_text() {
        let text = "Hello, world!";
        let (result, was_truncated) = truncate_text(text, 50_000);
        assert!(!was_truncated);
        assert_eq!(result, text);
    }

    #[test]
    fn should_handle_utf8_truncation_safely() {
        let text = "Hello ".to_string() + &"\u{20AC}".repeat(20_000);
        let (truncated, was_truncated) = truncate_text(&text, 100);
        assert!(was_truncated);
        assert!(!truncated.is_empty());
        assert!(truncated.contains("[... truncated"));
    }
}
