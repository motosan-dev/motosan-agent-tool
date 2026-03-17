use std::future::Future;
use std::pin::Pin;

use serde::Deserialize;
use serde_json::json;

use crate::{Tool, ToolContext, ToolDef, ToolResult};

/// A tool that generates PDF files from plain text or basic Markdown content.
///
/// Includes path traversal protection (rejects paths containing `..`).
pub struct GeneratePdfTool;

#[derive(Debug, Deserialize)]
struct GeneratePdfInput {
    content: String,
    output_path: String,
    #[serde(default = "default_format")]
    format: String,
    title: Option<String>,
}

fn default_format() -> String {
    "text".to_string()
}

impl Default for GeneratePdfTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GeneratePdfTool {
    pub fn new() -> Self {
        Self
    }
}

/// Strip basic Markdown formatting into plain text lines with simple style hints.
///
/// Returns a list of `(level, text)` tuples where `level` indicates:
///   0 = normal paragraph
///   1..=6 = heading level
fn parse_markdown_lines(content: &str) -> Vec<(u8, String)> {
    let mut result = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            result.push((0, String::new()));
            continue;
        }

        // Detect headings: # Heading
        let heading_level = trimmed.chars().take_while(|&c| c == '#').count();
        if heading_level > 0 && heading_level <= 6 {
            let text = trimmed[heading_level..].trim().to_string();
            // Strip inline formatting: **bold**, *italic*, `code`
            let text = strip_inline_formatting(&text);
            result.push((heading_level as u8, text));
        } else {
            let text = strip_inline_formatting(trimmed);
            result.push((0, text));
        }
    }
    result
}

/// Remove basic inline Markdown: **bold**, *italic*, `code`, [text](url) -> text
fn strip_inline_formatting(s: &str) -> String {
    let mut out = s.to_string();
    // Bold: **text** or __text__
    while let Some(start) = out.find("**") {
        if let Some(end) = out[start + 2..].find("**") {
            let inner = out[start + 2..start + 2 + end].to_string();
            out = format!("{}{}{}", &out[..start], inner, &out[start + 2 + end + 2..]);
        } else {
            break;
        }
    }
    // Italic: *text* (single)
    while let Some(start) = out.find('*') {
        if let Some(end) = out[start + 1..].find('*') {
            let inner = out[start + 1..start + 1 + end].to_string();
            out = format!("{}{}{}", &out[..start], inner, &out[start + 1 + end + 1..]);
        } else {
            break;
        }
    }
    // Inline code: `code`
    while let Some(start) = out.find('`') {
        if let Some(end) = out[start + 1..].find('`') {
            let inner = out[start + 1..start + 1 + end].to_string();
            out = format!("{}{}{}", &out[..start], inner, &out[start + 1 + end + 1..]);
        } else {
            break;
        }
    }
    // Links: [text](url) -> text
    while let Some(start) = out.find('[') {
        if let Some(mid) = out[start..].find("](") {
            let mid_abs = start + mid;
            if let Some(end) = out[mid_abs + 2..].find(')') {
                let link_text = out[start + 1..mid_abs].to_string();
                out = format!(
                    "{}{}{}",
                    &out[..start],
                    link_text,
                    &out[mid_abs + 2 + end + 1..]
                );
            } else {
                break;
            }
        } else {
            break;
        }
    }
    out
}

/// Render content into a PDF document and save to disk.
///
/// Uses the built-in Helvetica font (no external font files needed).
fn render_pdf(input: &GeneratePdfInput) -> std::result::Result<(u32, u64), String> {
    use printpdf::*;

    let title = input.title.as_deref().unwrap_or("Document");
    let (doc, page_idx, layer_idx) =
        PdfDocument::new(title, Mm(210.0), Mm(297.0), "Layer 1");

    let font_regular = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("Failed to load font: {e}"))?;
    let font_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("Failed to load bold font: {e}"))?;

    let page_width_mm = 210.0_f32;
    let page_height_mm = 297.0_f32;
    let margin_mm = 20.0_f32;
    let usable_width_mm = page_width_mm - 2.0 * margin_mm;

    let normal_size = 11.0_f32;
    let line_height_mm = 5.0_f32;
    let heading_sizes: [f32; 7] = [0.0, 22.0, 18.0, 15.0, 13.0, 12.0, 11.0];
    let heading_line_heights: [f32; 7] = [0.0, 10.0, 8.0, 7.0, 6.5, 6.0, 5.5];

    let lines = if input.format == "markdown" {
        parse_markdown_lines(&input.content)
    } else {
        input
            .content
            .lines()
            .map(|l| (0u8, l.to_string()))
            .collect()
    };

    // Estimate chars per line for word wrapping (approximate for Helvetica).
    let chars_per_line = |font_size: f32| -> usize {
        // Helvetica averages ~0.5 * font_size in mm per character.
        let char_width_mm = font_size * 0.25;
        (usable_width_mm / char_width_mm).max(1.0) as usize
    };

    let mut current_page = doc.get_page(page_idx);
    let mut current_layer = current_page.get_layer(layer_idx);
    let mut y = page_height_mm - margin_mm;
    let mut page_count: u32 = 1;

    // Helper closure -- we can't use closures that capture &mut doc easily,
    // so we collect all text operations and flush per page.
    let add_new_page =
        |doc: &PdfDocumentReference, page_count: &mut u32| -> (PdfPageIndex, PdfLayerIndex) {
            *page_count += 1;
            doc.add_page(Mm(210.0), Mm(297.0), "Layer 1")
        };

    for (level, text) in &lines {
        let level = *level;

        if text.is_empty() {
            y -= line_height_mm;
            if y < margin_mm {
                let (pi, li) = add_new_page(&doc, &mut page_count);
                current_page = doc.get_page(pi);
                current_layer = current_page.get_layer(li);
                y = page_height_mm - margin_mm;
            }
            continue;
        }

        let (font_size, lh, font) = if level > 0 {
            (
                heading_sizes[level as usize],
                heading_line_heights[level as usize],
                &font_bold,
            )
        } else {
            (normal_size, line_height_mm, &font_regular)
        };

        let max_chars = chars_per_line(font_size);

        // Simple word-wrap.
        let wrapped = word_wrap(text, max_chars);

        // Add extra spacing before headings.
        if level > 0 {
            y -= lh * 0.5;
        }

        for wrap_line in &wrapped {
            if y < margin_mm {
                let (pi, li) = add_new_page(&doc, &mut page_count);
                current_page = doc.get_page(pi);
                current_layer = current_page.get_layer(li);
                y = page_height_mm - margin_mm;
            }

            current_layer.use_text(wrap_line.as_str(), font_size, Mm(margin_mm), Mm(y), font);
            y -= lh;
        }

        // Extra spacing after headings.
        if level > 0 {
            y -= lh * 0.3;
        }
    }

    let pdf_bytes = doc.save_to_bytes().map_err(|e| format!("Failed to save PDF: {e}"))?;
    let size_bytes = pdf_bytes.len() as u64;

    std::fs::write(&input.output_path, &pdf_bytes)
        .map_err(|e| format!("Failed to write file: {e}"))?;

    Ok((page_count, size_bytes))
}

/// Simple word-wrap: break text at word boundaries to fit within max_chars.
fn word_wrap(text: &str, max_chars: usize) -> Vec<String> {
    if text.len() <= max_chars {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= max_chars {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

impl Tool for GeneratePdfTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "generate_pdf".to_string(),
            description: "Generate a PDF file from plain text or basic Markdown content. \
                Returns the output path, page count, and file size."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The text or Markdown content to render into a PDF"
                    },
                    "output_path": {
                        "type": "string",
                        "description": "The file path where the PDF will be saved (e.g. /tmp/report.pdf)"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["text", "markdown"],
                        "description": "Input format: 'text' for plain text, 'markdown' for basic Markdown"
                    },
                    "title": {
                        "type": "string",
                        "description": "Document title (used in PDF metadata)"
                    }
                },
                "required": ["content", "output_path"]
            }),
        }
    }

    fn call(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + '_>> {
        Box::pin(async move {
            let input: GeneratePdfInput = match serde_json::from_value(args) {
                Ok(v) => v,
                Err(e) => return ToolResult::error(format!("Invalid input: {e}")),
            };

            // Path traversal protection.
            if input.output_path.contains("..") {
                return ToolResult::error(
                    "Path traversal detected: paths containing '..' are not allowed",
                );
            }

            // Validate format.
            if input.format != "text" && input.format != "markdown" {
                return ToolResult::error(format!(
                    "Unsupported format '{}'. Use 'text' or 'markdown'.",
                    input.format
                ));
            }

            // Ensure parent directory exists.
            if let Some(parent) = std::path::Path::new(&input.output_path).parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    return ToolResult::error(format!(
                        "Parent directory does not exist: {}",
                        parent.display()
                    ));
                }
            }

            match render_pdf(&input) {
                Ok((pages, size_bytes)) => ToolResult::json(json!({
                    "path": input.output_path,
                    "pages": pages,
                    "size_bytes": size_bytes
                })),
                Err(e) => ToolResult::error(e),
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
        let tool = GeneratePdfTool::new();
        let def = tool.def();
        assert_eq!(def.name, "generate_pdf");

        // Validate schema itself (must come before moving input_schema).
        def.validate_input_schema().unwrap();

        let schema = &def.input_schema;
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["content"].is_object());
        assert!(schema["properties"]["output_path"].is_object());
        assert!(schema["properties"]["format"].is_object());
        assert!(schema["properties"]["title"].is_object());
        assert_eq!(schema["required"], json!(["content", "output_path"]));
    }

    #[tokio::test]
    async fn should_fail_with_invalid_input() {
        let tool = GeneratePdfTool::new();
        let ctx = test_ctx();
        let input = json!({"not_content": "test"});
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Invalid input"));
    }

    #[tokio::test]
    async fn should_reject_path_traversal() {
        let tool = GeneratePdfTool::new();
        let ctx = test_ctx();
        let input = json!({
            "content": "Hello",
            "output_path": "/tmp/../etc/evil.pdf"
        });
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Path traversal"));
    }

    #[tokio::test]
    async fn should_generate_text_pdf() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let output = dir.path().join("test.pdf");

        let tool = GeneratePdfTool::new();
        let ctx = test_ctx();
        let input = json!({
            "content": "Hello, World!\nThis is a test PDF.",
            "output_path": output.to_str().unwrap(),
            "title": "Test Document"
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error, "Expected success but got: {:?}", result);
        assert!(output.exists(), "PDF file should exist");

        let metadata = std::fs::metadata(&output).expect("read metadata");
        assert!(metadata.len() > 0, "PDF file should not be empty");

        // Check the PDF starts with %PDF header.
        let bytes = std::fs::read(&output).expect("read pdf");
        assert!(
            bytes.starts_with(b"%PDF"),
            "File should start with PDF header"
        );
    }

    #[tokio::test]
    async fn should_generate_markdown_pdf() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let output = dir.path().join("markdown.pdf");

        let tool = GeneratePdfTool::new();
        let ctx = test_ctx();
        let input = json!({
            "content": "# Report Title\n\nThis is a **bold** paragraph.\n\n## Section 1\n\nSome text with *italic* words.\n\n- bullet one\n- bullet two",
            "output_path": output.to_str().unwrap(),
            "format": "markdown",
            "title": "Test Markdown"
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error, "Expected success but got: {:?}", result);
        assert!(output.exists(), "PDF file should exist");

        let bytes = std::fs::read(&output).expect("read pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[tokio::test]
    async fn should_fail_for_missing_parent_directory() {
        let tool = GeneratePdfTool::new();
        let ctx = test_ctx();
        let input = json!({
            "content": "Hello",
            "output_path": "/nonexistent/dir/test.pdf"
        });
        let result = tool.call(input, &ctx).await;

        assert!(result.is_error);
        assert!(result.as_text().unwrap().contains("Parent directory"));
    }

    #[tokio::test]
    async fn should_return_json_with_metadata() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let output = dir.path().join("meta.pdf");

        let tool = GeneratePdfTool::new();
        let ctx = test_ctx();
        let input = json!({
            "content": "Page content",
            "output_path": output.to_str().unwrap()
        });
        let result = tool.call(input, &ctx).await;

        assert!(!result.is_error);
        // Check the JSON result has expected fields.
        let json_content = result.content.first().expect("should have content");
        match json_content {
            crate::ToolContent::Json(v) => {
                assert!(v["path"].is_string());
                assert!(v["pages"].is_number());
                assert!(v["size_bytes"].is_number());
                assert_eq!(v["pages"].as_u64().unwrap(), 1);
            }
            _ => panic!("Expected JSON content"),
        }
    }

    #[test]
    fn word_wrap_short_line() {
        let result = word_wrap("short", 80);
        assert_eq!(result, vec!["short"]);
    }

    #[test]
    fn word_wrap_long_line() {
        let result = word_wrap("hello world this is a long line", 12);
        assert!(result.len() > 1);
        for line in &result {
            // Each line should be <= max_chars (unless a single word exceeds it).
            assert!(line.len() <= 20, "line too long: {}", line);
        }
    }

    #[test]
    fn strip_inline_formatting_removes_markdown() {
        assert_eq!(strip_inline_formatting("**bold**"), "bold");
        assert_eq!(strip_inline_formatting("*italic*"), "italic");
        assert_eq!(strip_inline_formatting("`code`"), "code");
        assert_eq!(
            strip_inline_formatting("[link](https://example.com)"),
            "link"
        );
    }

    #[test]
    fn parse_markdown_lines_detects_headings() {
        let lines = parse_markdown_lines("# Title\n\nParagraph\n## Sub");
        assert_eq!(lines[0], (1, "Title".to_string()));
        assert_eq!(lines[1], (0, String::new()));
        assert_eq!(lines[2], (0, "Paragraph".to_string()));
        assert_eq!(lines[3], (2, "Sub".to_string()));
    }
}
