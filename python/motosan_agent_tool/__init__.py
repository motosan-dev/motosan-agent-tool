"""motosan-agent-tool -- Shared AI agent tool kit for LLM agents."""

from .error import ErrorKind, ToolError
from .registry import ToolRegistry
from .tool import (
    JsonContent,
    TextContent,
    Tool,
    ToolContent,
    ToolContext,
    ToolDef,
    ToolResult,
    tool_content_from_dict,
)

__all__ = [
    "ErrorKind",
    "JsonContent",
    "TextContent",
    "Tool",
    "ToolContent",
    "ToolContext",
    "ToolDef",
    "ToolError",
    "ToolRegistry",
    "ToolResult",
    "tool_content_from_dict",
]
