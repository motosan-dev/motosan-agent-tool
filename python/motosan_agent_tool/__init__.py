"""motosan-agent-tool -- Shared AI agent tool kit for LLM agents."""

from .error import ErrorKind, ToolError
from .registry import ToolRegistry
from .tool import (
    FunctionTool,
    JsonContent,
    TextContent,
    Tool,
    ToolContent,
    ToolContext,
    ToolDef,
    ToolResult,
    tool,
    tool_content_from_dict,
)

__all__ = [
    "ErrorKind",
    "FunctionTool",
    "JsonContent",
    "TextContent",
    "Tool",
    "ToolContent",
    "ToolContext",
    "ToolDef",
    "ToolError",
    "ToolRegistry",
    "ToolResult",
    "tool",
    "tool_content_from_dict",
]
