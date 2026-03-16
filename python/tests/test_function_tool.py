"""Tests for FunctionTool and @tool decorator."""

from __future__ import annotations

from typing import Any

import pytest
import pytest_asyncio  # noqa: F401

from motosan_agent_tool import (
    ErrorKind,
    FunctionTool,
    ToolContext,
    ToolError,
    ToolResult,
    tool,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

WEATHER_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {"city": {"type": "string"}},
    "required": ["city"],
}


async def _get_weather(args: dict[str, Any], ctx: ToolContext) -> ToolResult:
    _ = ctx
    return ToolResult.text(f"Sunny in {args['city']}")


# ---------------------------------------------------------------------------
# FunctionTool (class)
# ---------------------------------------------------------------------------


class TestFunctionTool:
    def test_def(self) -> None:
        ft = FunctionTool(
            name="get_weather",
            description="Get weather",
            input_schema=WEATHER_SCHEMA,
            fn=_get_weather,
        )
        d = ft.def_()
        assert d.name == "get_weather"
        assert d.description == "Get weather"
        assert d.input_schema == WEATHER_SCHEMA

    @pytest.mark.asyncio
    async def test_call(self) -> None:
        ft = FunctionTool(
            name="get_weather",
            description="Get weather",
            input_schema=WEATHER_SCHEMA,
            fn=_get_weather,
        )
        ctx = ToolContext.new("test", "unit")
        result = await ft.call({"city": "Taipei"}, ctx)
        assert not result.is_error
        assert result.as_text() == "Sunny in Taipei"

    @pytest.mark.asyncio
    async def test_validates_args(self) -> None:
        ft = FunctionTool(
            name="get_weather",
            description="Get weather",
            input_schema=WEATHER_SCHEMA,
            fn=_get_weather,
        )
        ctx = ToolContext.new("test", "unit")
        with pytest.raises(ToolError) as exc:
            await ft.call({}, ctx)
        assert exc.value.kind == ErrorKind.MISSING_FIELD

    @pytest.mark.asyncio
    async def test_validates_type(self) -> None:
        ft = FunctionTool(
            name="get_weather",
            description="Get weather",
            input_schema=WEATHER_SCHEMA,
            fn=_get_weather,
        )
        ctx = ToolContext.new("test", "unit")
        with pytest.raises(ToolError) as exc:
            await ft.call({"city": 123}, ctx)
        assert exc.value.kind == ErrorKind.VALIDATION


# ---------------------------------------------------------------------------
# @tool decorator
# ---------------------------------------------------------------------------


@tool(
    name="get_weather",
    description="Get weather",
    input_schema=WEATHER_SCHEMA,
)
async def get_weather(args: dict[str, Any], ctx: ToolContext) -> ToolResult:
    _ = ctx
    return ToolResult.text(f"Sunny in {args['city']}")


class TestToolDecorator:
    def test_returns_function_tool(self) -> None:
        assert isinstance(get_weather, FunctionTool)

    def test_def(self) -> None:
        d = get_weather.def_()
        assert d.name == "get_weather"
        assert d.description == "Get weather"

    @pytest.mark.asyncio
    async def test_call(self) -> None:
        ctx = ToolContext.new("test", "unit")
        result = await get_weather.call({"city": "Taipei"}, ctx)
        assert not result.is_error
        assert result.as_text() == "Sunny in Taipei"

    @pytest.mark.asyncio
    async def test_rejects_missing_required(self) -> None:
        ctx = ToolContext.new("test", "unit")
        with pytest.raises(ToolError):
            await get_weather.call({}, ctx)

    def test_def_validation_rejects_missing_required(self) -> None:
        with pytest.raises(ToolError):
            get_weather.def_().validate_args({})


# ---------------------------------------------------------------------------
# ToolRegistry integration
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_function_tool_in_registry() -> None:
    """FunctionTool instances work with ToolRegistry."""
    from motosan_agent_tool import ToolRegistry

    registry = ToolRegistry()
    await registry.register(get_weather)  # type: ignore[arg-type]
    assert await registry.len() == 1

    defs = await registry.list_defs()
    assert defs[0].name == "get_weather"

    t = await registry.get("get_weather")
    assert t is not None
    ctx = ToolContext.new("test", "unit")
    result = await t.call({"city": "Tokyo"}, ctx)
    assert result.as_text() == "Sunny in Tokyo"
