"""Expose Parallel Search MCP tools through a short-lived ToolRegistry."""

from __future__ import annotations

import argparse
import asyncio
import json
from contextlib import asynccontextmanager
from datetime import timedelta
from typing import AsyncIterator

import httpx
from mcp import ClientSession, types
from mcp.client.streamable_http import streamable_http_client
from motosan_agent_tool import (
    JsonContent,
    TextContent,
    Tool,
    ToolContext,
    ToolDef,
    ToolRegistry,
    ToolResult,
)

ENDPOINT = "https://search.parallel.ai/mcp"
USER_AGENT = "motosan-agent-tool/parallel-search-example"
TIMEOUT_SECONDS = 60
REMOTE_TOOLS = {"web_search", "web_fetch"}


class ParallelTool(Tool):
    """Adapt one discovered MCP tool while keeping its schema and error flag."""

    def __init__(self, session: ClientSession, definition: types.Tool) -> None:
        self.session = session
        self.remote_name = definition.name
        self.definition = ToolDef(
            name=f"parallel_{definition.name}",
            description=definition.description or definition.name,
            input_schema=definition.inputSchema,
        )

    def def_(self) -> ToolDef:
        return self.definition

    async def call(self, args: dict, ctx: ToolContext) -> ToolResult:
        self.definition.validate_args(args)
        try:
            response = await asyncio.wait_for(
                self.session.call_tool(self.remote_name, arguments=args),
                timeout=TIMEOUT_SECONDS,
            )
        except asyncio.TimeoutError:
            return ToolResult.error(f"Parallel request timed out after {TIMEOUT_SECONDS:g} seconds")
        except Exception as exc:
            # CancelledError deliberately propagates to the caller.
            return ToolResult.error(f"Parallel request failed: {exc}")

        content = [
            TextContent(block.text)
            if isinstance(block, types.TextContent)
            else JsonContent(block.model_dump(mode="json", by_alias=True))
            for block in response.content
        ]
        if response.structuredContent is not None:
            content.append(JsonContent(response.structuredContent))
        return ToolResult(content=content, is_error=response.isError)


@asynccontextmanager
async def parallel_registry() -> AsyncIterator[ToolRegistry]:
    """Create an isolated registry; its tools are valid only inside this context."""
    async with httpx.AsyncClient(
        headers={"User-Agent": USER_AGENT},
        timeout=httpx.Timeout(TIMEOUT_SECONDS),
        follow_redirects=False,
    ) as client:
        async with streamable_http_client(ENDPOINT, http_client=client) as streams:
            async with ClientSession(
                streams[0], streams[1],
                read_timeout_seconds=timedelta(seconds=TIMEOUT_SECONDS),
            ) as session:
                await session.initialize()
                registry = ToolRegistry()
                found = set()
                cursor = None
                while True:
                    page = await session.list_tools(cursor=cursor)
                    for definition in page.tools:
                        if definition.name in REMOTE_TOOLS:
                            await registry.register(ParallelTool(session, definition))
                            found.add(definition.name)
                    cursor = page.nextCursor
                    if not cursor:
                        break
                if found != REMOTE_TOOLS:
                    raise RuntimeError("Parallel MCP did not advertise search and fetch")
                yield registry


async def run(args: argparse.Namespace) -> int:
    if args.command == "search":
        name = "parallel_web_search"
        arguments = {"objective": args.query, "search_queries": [args.query]}
    else:
        name = "parallel_web_fetch"
        arguments = {"urls": args.urls}
    if args.session_id:
        arguments["session_id"] = args.session_id

    async with parallel_registry() as registry:
        tool = await registry.get(name)
        assert tool is not None
        result = await tool.call(arguments, ToolContext.new("example", "cli"))
        for block in result.content:
            print(block.text if isinstance(block, TextContent) else json.dumps(block.data))
        return 1 if result.is_error else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session-id", help="Reuse one ID for related search/fetch calls")
    commands = parser.add_subparsers(dest="command", required=True)
    search = commands.add_parser("search", help="Search the web for current information")
    search.add_argument("query")
    fetch = commands.add_parser("fetch", help="Extract content from public web pages")
    fetch.add_argument("urls", nargs="+")
    return asyncio.run(run(parser.parse_args()))


if __name__ == "__main__":
    raise SystemExit(main())
