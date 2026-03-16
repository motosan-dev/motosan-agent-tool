"""Tests for motosan_agent_tool.registry -- mirrors the Rust test suite."""

from __future__ import annotations

from typing import Any

import pytest

from motosan_agent_tool import Tool, ToolContext, ToolDef, ToolResult
from motosan_agent_tool.registry import ToolRegistry


# ---------------------------------------------------------------------------
# Test tool
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


class UpperTool(Tool):
    def def_(self) -> ToolDef:
        return ToolDef(
            name="upper",
            description="Uppercase text",
            input_schema={
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
            },
        )

    async def call(self, args: dict[str, Any], ctx: ToolContext) -> ToolResult:
        return ToolResult.text(args.get("text", "").upper())


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestToolRegistry:
    @pytest.mark.asyncio
    async def test_register_and_get(self) -> None:
        registry = ToolRegistry()
        await registry.register(EchoTool())
        assert await registry.len() == 1

        tool = await registry.get("echo")
        assert tool is not None
        assert tool.def_().name == "echo"

    @pytest.mark.asyncio
    async def test_list_defs_sorted(self) -> None:
        registry = ToolRegistry()
        await registry.register(UpperTool())
        await registry.register(EchoTool())

        defs = await registry.list_defs()
        assert len(defs) == 2
        assert defs[0].name == "echo"
        assert defs[1].name == "upper"

    @pytest.mark.asyncio
    async def test_get_missing_returns_none(self) -> None:
        registry = ToolRegistry()
        assert await registry.get("missing") is None

    @pytest.mark.asyncio
    async def test_deregister_removes_tool(self) -> None:
        registry = ToolRegistry()
        await registry.register(EchoTool())
        assert await registry.len() == 1

        removed = await registry.deregister("echo")
        assert removed is not None
        assert removed.def_().name == "echo"
        assert await registry.len() == 0

    @pytest.mark.asyncio
    async def test_deregister_missing_returns_none(self) -> None:
        registry = ToolRegistry()
        assert await registry.deregister("missing") is None

    @pytest.mark.asyncio
    async def test_clear_removes_all(self) -> None:
        registry = ToolRegistry()
        await registry.register(EchoTool())
        assert not await registry.is_empty()

        await registry.clear()
        assert await registry.is_empty()

    @pytest.mark.asyncio
    async def test_is_empty_on_new(self) -> None:
        registry = ToolRegistry()
        assert await registry.is_empty()

    @pytest.mark.asyncio
    async def test_register_overwrites(self) -> None:
        registry = ToolRegistry()
        await registry.register(EchoTool())
        await registry.register(EchoTool())
        assert await registry.len() == 1
