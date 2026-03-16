use std::future::Future;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

const DEFAULT_MAX_CHARS: usize = 5000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 1_048_576; // 1 MB

/// A tool that fetches a web page and extracts readable text content.
pub struct FetchUrlTool {
    http: Client,
}

#[derive(Debug, Deserialize)]
struct FetchUrlInput {
    url: String,
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
            v4.is_loopback()          // 127.0.0.0/8
                || v4.is_private()    // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local() // 169.254.0.0/16
                || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

/// Extract the content of the first occurrence of a given tag from HTML.
fn extract_tag_content(html: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = html.find(&open)?;
    let after_open = html[start..].find('>')? + start + 1;
    let end = html[after_open..].find(&close)? + after_open;
    Some(html[after_open..end].to_string())
}

/// Strip HTML tags and collapse whitespace to produce readable text.
fn strip_html(html: &str) -> String {
    let mut result = html.to_string();
    for tag in &["script", "style", "nav", "footer", "header"] {
        loop {
            let open = format!("<{}", tag);
            let close = format!("</{}>", tag);
            if let Some(start) = result.to_lowercase().find(&open) {
                if let Some(end_offset) = result.to_lowercase()[start..].find(&close) {
                    let end = start + end_offset + close.len();
                    result = format!("{}{}", &result[..start], &result[end..]);
                    continue;
                }
            }
            break;
        }
    }

    // Strip remaining HTML tags.
    let mut out = String::with_capacity(result.len());
    let mut in_tag = false;
    for ch in result.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }

    // Decode common HTML entities.
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace.
    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

impl Default for FetchUrlTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FetchUrlTool {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("failed to build HTTP client"),
        }
    }
}

impl Tool for FetchUrlTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "fetch_url".to_string(),
            description: "Fetch a web page and extract its readable text content. \
                Strips navigation, ads, scripts, and HTML tags to return clean text."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch (must start with http:// or https://)"
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Maximum number of characters to return (default: 5000)",
                        "default": 5000
                    }
                },
                "required": ["url"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let start = std::time::Instant::now();

            let mut input: FetchUrlInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            if input.max_chars == 0 {
                input.max_chars = DEFAULT_MAX_CHARS;
            }

            // Validate URL scheme.
            if !input.url.starts_with("http://") && !input.url.starts_with("https://") {
                return ToolResult::error("Invalid URL: must start with http:// or https://");
            }

            // SSRF protection.
            if let Err(e) = check_ssrf(&input.url) {
                return ToolResult::error(e);
            }

            let response = match self
                .http
                .get(&input.url)
                .header("User-Agent", "Mozilla/5.0 (compatible; MotosanAgent/1.0)")
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::error(format!("Failed to fetch URL: {e}")),
            };

            if !response.status().is_success() {
                let status = response.status();
                return ToolResult::error(format!(
                    "HTTP error {status} when fetching {}",
                    input.url
                ));
            }

            if let Some(cl) = response.content_length() {
                if cl as usize > MAX_RESPONSE_BYTES {
                    return ToolResult::error(format!(
                        "Response too large: {cl} bytes exceeds {MAX_RESPONSE_BYTES} byte limit"
                    ));
                }
            }

            let bytes = match response.bytes().await {
                Ok(b) => b,
                Err(e) => return ToolResult::error(format!("Failed to read response body: {e}")),
            };
            if bytes.len() > MAX_RESPONSE_BYTES {
                return ToolResult::error(format!(
                    "Response too large: {} bytes exceeds {MAX_RESPONSE_BYTES} byte limit",
                    bytes.len()
                ));
            }

            let html = String::from_utf8_lossy(&bytes).into_owned();

            let title = extract_tag_content(&html, "title")
                .map(|t| strip_html(&t))
                .unwrap_or_default();

            let mut content = strip_html(&html);

            // UTF-8 safe truncation.
            if content.len() > input.max_chars {
                let safe_boundary = content
                    .char_indices()
                    .map(|(idx, _)| idx)
                    .take_while(|&idx| idx <= input.max_chars)
                    .last()
                    .unwrap_or(0);
                if let Some(last_space) = content[..safe_boundary].rfind(' ') {
                    content = format!("{}...", &content[..last_space]);
                } else {
                    content = format!("{}...", &content[..safe_boundary]);
                }
            }

            let duration = start.elapsed().as_millis() as u64;

            let text = if title.is_empty() {
                content
            } else {
                format!("Title: {title}\n\n{content}")
            };

            ToolResult::text(text)
                .with_citation(input.url)
                .with_duration(duration)
        })
    }
}

/// Resolve hostname and block private/reserved IPs (SSRF protection).
fn check_ssrf(url: &str) -> Result<(), String> {
    // Parse out host + port.
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new("test-agent", "test")
    }

    #[test]
    fn should_have_correct_name_and_schema() {
        let tool = FetchUrlTool::new();
        let def = tool.def();
        assert_eq!(def.name, "fetch_url");

        let schema = def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["url"].is_object());
        assert!(schema["properties"]["max_chars"].is_object());
        assert_eq!(schema["required"], json!(["url"]));
    }

    #[tokio::test]
    async fn should_reject_invalid_url_scheme() {
        let tool = FetchUrlTool::new();
        let ctx = test_ctx();
        let input = json!({"url": "ftp://example.com"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("must start with http"));
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = FetchUrlTool::new();
        let ctx = test_ctx();
        let input = json!({"not_url": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[test]
    fn should_strip_html_tags() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("<h1>"));
    }

    #[test]
    fn should_strip_script_and_style() {
        let html = r#"<html><head><style>body{color:red}</style></head>
            <body><script>alert('hi')</script><p>Content here</p></body></html>"#;
        let text = strip_html(html);
        assert!(text.contains("Content here"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("color:red"));
    }

    #[test]
    fn should_extract_title() {
        let html = "<html><head><title>My Page Title</title></head><body></body></html>";
        let title = extract_tag_content(html, "title").unwrap();
        assert_eq!(title, "My Page Title");
    }

    #[tokio::test]
    async fn should_block_private_ip_localhost() {
        let tool = FetchUrlTool::new();
        let ctx = test_ctx();
        let input = json!({"url": "http://127.0.0.1/secret"});
        let result = tool.call(input, &ctx).await;
        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("private/reserved IP"));
    }

    #[test]
    fn should_detect_private_ips() {
        use std::net::IpAddr;
        assert!(is_private_ip("127.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("10.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("172.16.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("192.168.1.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("169.254.1.1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("::1".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip("8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip("1.1.1.1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn should_handle_utf8_truncation_safely() {
        let content = "Hello \u{20AC}\u{20AC}\u{20AC}\u{20AC}\u{20AC}\u{20AC}\u{20AC}\u{20AC}\u{20AC}\u{20AC} world";
        let max_chars: usize = 10;
        let safe_boundary = content
            .char_indices()
            .map(|(idx, _)| idx)
            .take_while(|&idx| idx <= max_chars)
            .last()
            .unwrap_or(0);
        let truncated = &content[..safe_boundary];
        assert!(truncated.len() <= max_chars);
    }
}
