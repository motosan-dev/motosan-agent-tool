"""Tests for motosan_agent_tool.tool -- mirrors the Rust test suite."""

from __future__ import annotations

from typing import Any

import pytest
import pytest_asyncio  # noqa: F401 (ensures plugin is loaded)

from motosan_agent_tool import (
    ErrorKind,
    JsonContent,
    TextContent,
    Tool,
    ToolContext,
    ToolDef,
    ToolError,
    ToolResult,
    tool_content_from_dict,
)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


def search_def() -> ToolDef:
    return ToolDef(
        name="web_search",
        description="Search the web",
        input_schema={
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "max_results": {"type": "integer"},
            },
            "required": ["query"],
        },
    )


# ---------------------------------------------------------------------------
# ToolDef -- validate_input_schema
# ---------------------------------------------------------------------------


class TestToolDefValidateSchema:
    def test_accepts_valid(self) -> None:
        search_def().validate_input_schema()

    def test_rejects_non_dict(self) -> None:
        d = ToolDef(name="x", description="x", input_schema="not a dict")  # type: ignore[arg-type]
        with pytest.raises(ToolError) as exc:
            d.validate_input_schema()
        assert exc.value.kind == ErrorKind.VALIDATION

    def test_rejects_missing_properties(self) -> None:
        d = ToolDef(name="bad", description="bad", input_schema={"type": "object"})
        with pytest.raises(ToolError) as exc:
            d.validate_input_schema()
        assert exc.value.kind == ErrorKind.VALIDATION

    def test_rejects_wrong_type(self) -> None:
        d = ToolDef(
            name="bad",
            description="bad",
            input_schema={"type": "array", "properties": {}},
        )
        with pytest.raises(ToolError) as exc:
            d.validate_input_schema()
        assert exc.value.kind == ErrorKind.VALIDATION

    def test_rejects_required_not_in_properties(self) -> None:
        d = ToolDef(
            name="bad",
            description="bad",
            input_schema={
                "type": "object",
                "properties": {"a": {"type": "string"}},
                "required": ["b"],
            },
        )
        with pytest.raises(ToolError):
            d.validate_input_schema()

    def test_rejects_non_string_required_entries(self) -> None:
        d = ToolDef(
            name="bad",
            description="bad",
            input_schema={
                "type": "object",
                "properties": {},
                "required": [123],
            },
        )
        with pytest.raises(ToolError):
            d.validate_input_schema()

    def test_rejects_required_not_array(self) -> None:
        d = ToolDef(
            name="bad",
            description="bad",
            input_schema={
                "type": "object",
                "properties": {},
                "required": "not_a_list",
            },
        )
        with pytest.raises(ToolError):
            d.validate_input_schema()


# ---------------------------------------------------------------------------
# ToolDef -- validate_args
# ---------------------------------------------------------------------------


class TestToolDefValidateArgs:
    def test_accepts_valid(self) -> None:
        search_def().validate_args({"query": "rust"})

    def test_rejects_missing_required(self) -> None:
        with pytest.raises(ToolError) as exc:
            search_def().validate_args({"max_results": 5})
        assert exc.value.kind == ErrorKind.MISSING_FIELD

    def test_rejects_wrong_type(self) -> None:
        with pytest.raises(ToolError) as exc:
            search_def().validate_args({"query": 123})
        assert exc.value.kind == ErrorKind.VALIDATION

    def test_rejects_non_dict_args(self) -> None:
        with pytest.raises(ToolError):
            search_def().validate_args("not a dict")  # type: ignore[arg-type]

    def test_checks_enum_pass(self) -> None:
        d = ToolDef(
            name="t",
            description="t",
            input_schema={
                "type": "object",
                "properties": {
                    "lang": {"type": "string", "enum": ["en", "ja", "zh"]},
                },
                "required": ["lang"],
            },
        )
        d.validate_args({"lang": "en"})

    def test_checks_enum_fail(self) -> None:
        d = ToolDef(
            name="t",
            description="t",
            input_schema={
                "type": "object",
                "properties": {
                    "lang": {"type": "string", "enum": ["en", "ja", "zh"]},
                },
                "required": ["lang"],
            },
        )
        with pytest.raises(ToolError):
            d.validate_args({"lang": "fr"})

    def test_boolean_not_integer(self) -> None:
        """Python bool is subclass of int -- make sure we reject it for integer fields."""
        with pytest.raises(ToolError):
            search_def().validate_args({"query": "x", "max_results": True})

    def test_ignores_unknown_properties(self) -> None:
        """Extra args not in properties should be silently accepted."""
        search_def().validate_args({"query": "rust", "unknown_field": 42})

    @pytest.mark.parametrize(
        "type_name,good,bad",
        [
            ("string", "hello", 42),
            ("number", 3.14, "nope"),
            ("integer", 7, "nope"),
            ("boolean", True, "nope"),
            ("object", {"a": 1}, "nope"),
            ("array", [1, 2], "nope"),
            ("null", None, "nope"),
        ],
    )
    def test_type_checking(self, type_name: str, good: Any, bad: Any) -> None:
        d = ToolDef(
            name="t",
            description="t",
            input_schema={
                "type": "object",
                "properties": {"v": {"type": type_name}},
                "required": [],
            },
        )
        d.validate_args({"v": good})
        with pytest.raises(ToolError):
            d.validate_args({"v": bad})


# ---------------------------------------------------------------------------
# ToolDef -- serde
# ---------------------------------------------------------------------------


class TestToolDefSerde:
    def test_roundtrip(self) -> None:
        original = search_def()
        d = original.to_dict()
        restored = ToolDef.from_dict(d)
        assert original == restored

    def test_equality(self) -> None:
        assert search_def() == search_def()


# ---------------------------------------------------------------------------
# ToolResult
# ---------------------------------------------------------------------------


class TestToolResult:
    def test_text(self) -> None:
        r = ToolResult.text("hello")
        assert not r.is_error
        assert r.as_text() == "hello"
        assert r.citation is None

    def test_json(self) -> None:
        r = ToolResult.json({"key": "value"})
        assert not r.is_error
        assert r.as_text() is None
        assert isinstance(r.content[0], JsonContent)

    def test_error(self) -> None:
        r = ToolResult.error("boom")
        assert r.is_error
        assert r.as_text() == "boom"

    def test_builder_chain(self) -> None:
        r = (
            ToolResult.text("data")
            .with_citation("https://example.com")
            .with_inject(True)
            .with_duration(42)
        )
        assert r.citation == "https://example.com"
        assert r.inject_to_context is True
        assert r.duration_ms == 42

    def test_as_text_returns_none_for_json_only(self) -> None:
        r = ToolResult.json({"x": 1})
        assert r.as_text() is None


# ---------------------------------------------------------------------------
# ToolContent
# ---------------------------------------------------------------------------


class TestToolContent:
    def test_text_content_to_dict(self) -> None:
        c = TextContent("hi")
        d = c.to_dict()
        assert d == {"type": "text", "data": "hi"}

    def test_json_content_to_dict(self) -> None:
        c = JsonContent({"key": "value"})
        d = c.to_dict()
        assert d == {"type": "json", "data": {"key": "value"}}

    def test_text_roundtrip(self) -> None:
        original = TextContent("hello")
        restored = TextContent.from_dict(original.to_dict())
        assert original == restored

    def test_json_roundtrip(self) -> None:
        original = JsonContent({"key": "value"})
        restored = JsonContent.from_dict(original.to_dict())
        assert original == restored

    def test_tool_content_from_dict_text(self) -> None:
        c = tool_content_from_dict({"type": "text", "data": "hi"})
        assert isinstance(c, TextContent)
        assert c.text == "hi"

    def test_tool_content_from_dict_json(self) -> None:
        c = tool_content_from_dict({"type": "json", "data": [1, 2, 3]})
        assert isinstance(c, JsonContent)
        assert c.data == [1, 2, 3]

    def test_tool_content_from_dict_unknown(self) -> None:
        with pytest.raises(ToolError):
            tool_content_from_dict({"type": "image", "data": "bytes"})

    def test_tagged_format(self) -> None:
        """Mirrors Rust serde_tool_content_tagged_format test."""
        text = TextContent("hi")
        serialized = text.to_dict()
        assert serialized["type"] == "text"
        assert serialized["data"] == "hi"


# ---------------------------------------------------------------------------
# ToolContext
# ---------------------------------------------------------------------------


class TestToolContext:
    def test_new(self) -> None:
        ctx = ToolContext.new("agent-1", "crucible")
        assert ctx.caller_id == "agent-1"
        assert ctx.platform == "crucible"
        assert ctx.extra == {}

    def test_with_builder(self) -> None:
        ctx = ToolContext.new("agent-1", "crucible").with_("org_id", "motosan")
        assert ctx.extra["org_id"] == "motosan"

    def test_get_str(self) -> None:
        ctx = ToolContext.new("a", "b").with_("org_id", "motosan")
        assert ctx.get_str("org_id") == "motosan"
        assert ctx.get_str("missing") is None

    def test_get_u64(self) -> None:
        ctx = ToolContext.new("a", "b").with_("budget", 5)
        assert ctx.get_u64("budget") == 5
        assert ctx.get_u64("missing") is None

    def test_get_u64_rejects_bool(self) -> None:
        ctx = ToolContext.new("a", "b").with_("flag", True)
        assert ctx.get_u64("flag") is None

    def test_get_u64_rejects_negative(self) -> None:
        ctx = ToolContext.new("a", "b").with_("neg", -1)
        assert ctx.get_u64("neg") is None

    def test_get_bool(self) -> None:
        ctx = ToolContext.new("a", "b").with_("flag", True)
        assert ctx.get_bool("flag") is True
        assert ctx.get_bool("missing") is None

    def test_extra_helpers_combined(self) -> None:
        """Mirrors Rust tool_context_extra_helpers test."""
        ctx = (
            ToolContext.new("agent-1", "crucible")
            .with_("org_id", "motosan")
            .with_("budget", 5)
        )
        assert ctx.get_str("org_id") == "motosan"
        assert ctx.get_u64("budget") == 5
        assert ctx.get_str("missing") is None


# ---------------------------------------------------------------------------
# Tool ABC
# ---------------------------------------------------------------------------


class EchoTool(Tool):
    def def_(self) -> ToolDef:
        return ToolDef(
            name="echo",
            description="Echo back the input",
            input_schema={
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
            },
        )

    async def call(self, args: dict[str, Any], ctx: ToolContext) -> ToolResult:
        return ToolResult.text(args.get("text", ""))


class TestToolABC:
    def test_cannot_instantiate_abstract(self) -> None:
        with pytest.raises(TypeError):
            Tool()  # type: ignore[abstract]

    def test_concrete_def(self) -> None:
        tool = EchoTool()
        d = tool.def_()
        assert d.name == "echo"

    @pytest.mark.asyncio
    async def test_concrete_call(self) -> None:
        tool = EchoTool()
        ctx = ToolContext.new("test", "unit")
        result = await tool.call({"text": "hello"}, ctx)
        assert result.as_text() == "hello"
        assert not result.is_error
