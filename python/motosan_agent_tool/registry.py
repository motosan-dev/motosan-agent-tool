"""Async-safe tool registry mirroring the Rust ``ToolRegistry``."""

from __future__ import annotations

import asyncio

from .tool import Tool, ToolDef


class ToolRegistry:
    """Thread-safe (via ``asyncio.Lock``) registry of named tools.

    All public methods are ``async`` to mirror the Rust API which uses
    ``tokio::sync::RwLock``.
    """

    def __init__(self) -> None:
        self._tools: dict[str, Tool] = {}
        self._lock = asyncio.Lock()

    async def register(self, tool: Tool) -> None:
        """Register a tool. Overwrites any existing tool with the same name."""
        name = tool.def_().name
        async with self._lock:
            self._tools[name] = tool

    async def get(self, name: str) -> Tool | None:
        """Get a tool by name."""
        async with self._lock:
            return self._tools.get(name)

    async def list_defs(self) -> list[ToolDef]:
        """List all tool definitions, sorted by name for determinism."""
        async with self._lock:
            defs = [t.def_() for t in self._tools.values()]
        defs.sort(key=lambda d: d.name)
        return defs

    async def len(self) -> int:
        """Number of registered tools."""
        async with self._lock:
            return len(self._tools)

    async def is_empty(self) -> bool:
        """Whether the registry is empty."""
        async with self._lock:
            return len(self._tools) == 0

    async def deregister(self, name: str) -> Tool | None:
        """Remove a tool by name. Returns the removed tool, if any."""
        async with self._lock:
            return self._tools.pop(name, None)

    async def clear(self) -> None:
        """Remove all tools from the registry."""
        async with self._lock:
            self._tools.clear()
