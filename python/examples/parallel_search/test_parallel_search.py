"""Offline checks through the real MCP SDK and ToolRegistry using HTTP fixtures."""

import argparse
import asyncio
import io
import json
import unittest
from contextlib import redirect_stdout
from unittest.mock import patch

import httpx
from motosan_agent_tool import JsonContent, ToolContext, ToolError

import parallel_search as example


class RegistryTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.requests = []
        self.mode = "normal"
        self.started = asyncio.Event()
        self.release = asyncio.Event()
        client_type = httpx.AsyncClient

        def client_factory(**kwargs):
            client = client_type(transport=httpx.MockTransport(self.respond), **kwargs)
            self.client = client
            return client

        self.client_patch = patch.object(example.httpx, "AsyncClient", client_factory)
        self.client_patch.start()
        self.addCleanup(self.client_patch.stop)

    async def respond(self, request):
        self.requests.append(request)
        self.assertEqual(str(request.url), example.ENDPOINT)
        self.assertEqual(request.headers["user-agent"], example.USER_AGENT)
        self.assertNotIn("authorization", request.headers)
        payload = json.loads(request.content)
        method = payload["method"]
        if "id" not in payload:
            return httpx.Response(202)
        if method == "initialize":
            result = {
                "protocolVersion": payload["params"]["protocolVersion"],
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "parallel-fixture", "version": "1"},
            }
        elif method == "tools/list":
            if payload.get("params", {}).get("cursor"):
                result = {"tools": [self.definition("web_fetch")]}
            else:
                result = {
                    "tools": [self.definition("web_search"), self.definition("ignored")],
                    "nextCursor": "fetch-page",
                }
                if self.mode == "missing":
                    result.pop("nextCursor")
        elif method == "tools/call":
            self.started.set()
            if self.mode == "waiting":
                await self.release.wait()
            if self.mode == "network":
                raise httpx.ReadTimeout("fixture timeout", request=request)
            if self.mode == "protocol":
                return httpx.Response(200, json={
                    "jsonrpc": "2.0", "id": payload["id"],
                    "error": {"code": -32603, "message": "fixture server failure"},
                })
            result = {
                "content": [{"type": "text", "text": "Source: https://www.python.org/"}],
                "structuredContent": {"results": [{"url": "https://www.python.org/"}]},
                "isError": self.mode == "tool_error",
            }
            if self.mode == "image":
                result["content"].append({"type": "image", "data": "aGk=", "mimeType": "image/png"})
        else:
            self.fail(f"Unexpected method: {method}")
        return httpx.Response(200, json={"jsonrpc": "2.0", "id": payload["id"], "result": result})

    @staticmethod
    def definition(name):
        properties = (
            {"urls": {"type": "array"}}
            if name == "web_fetch"
            else {"objective": {"type": "string"}, "search_queries": {"type": "array"}}
        )
        return {
            "name": name, "description": f"Fixture {name}",
            "inputSchema": {"type": "object", "properties": properties, "required": list(properties)},
        }

    async def call(self, registry, name="web_search"):
        tool = await registry.get(f"parallel_{name}")
        args = {"urls": ["https://www.python.org/"]} if name == "web_fetch" else {
            "objective": "current Python release", "search_queries": ["current Python release"],
        }
        return await tool.call(args, ToolContext.new("test", "unittest"))

    async def test_discovery_dispatch_and_content(self):
        async with example.parallel_registry() as registry:
            self.assertEqual([d.name for d in await registry.list_defs()], [
                "parallel_web_fetch", "parallel_web_search",
            ])
            for name in ("web_search", "web_fetch"):
                result = await self.call(registry, name)
                self.assertFalse(result.is_error)
                self.assertIn("https://www.python.org/", result.as_text())
                self.assertEqual(result.content[-1].data["results"][0]["url"], "https://www.python.org/")
            calls = [json.loads(r.content)["params"] for r in self.requests
                     if json.loads(r.content)["method"] == "tools/call"]
            self.assertEqual(calls[0]["name"], "web_search")
            self.assertEqual(calls[0]["arguments"]["objective"], "current Python release")
            self.assertEqual(calls[1]["arguments"]["urls"], ["https://www.python.org/"])
            tool = await registry.get("parallel_web_search")
            with self.assertRaises(ToolError):
                await tool.call({}, ToolContext.new("test", "unittest"))
        self.assertTrue(self.client.is_closed)

    async def test_tool_and_protocol_errors(self):
        async with example.parallel_registry() as registry:
            self.mode = "tool_error"
            self.assertTrue((await self.call(registry)).is_error)
            self.mode = "protocol"
            result = await self.call(registry)
            self.assertTrue(result.is_error)
            self.assertIn("fixture server failure", result.as_text())

    async def test_network_failure(self):
        # Fatal transport failures may also surface on context teardown.
        with self.assertRaises(Exception) as raised:
            async with example.parallel_registry() as registry:
                self.mode = "network"
                result = await self.call(registry)
                self.assertTrue(result.is_error)

        def contains_timeout(error):
            return isinstance(error, httpx.ReadTimeout) or any(
                contains_timeout(child) for child in getattr(error, "exceptions", ())
            )

        self.assertTrue(contains_timeout(raised.exception))
        self.assertTrue(self.client.is_closed)

    async def test_other_content_blocks(self):
        async with example.parallel_registry() as registry:
            self.mode = "image"
            result = await self.call(registry)
            self.assertIsInstance(result.content[1], JsonContent)
            self.assertEqual(result.content[1].data["mimeType"], "image/png")

    async def test_timeout(self):
        async with example.parallel_registry() as registry:
            self.mode = "waiting"
            with patch.object(example, "TIMEOUT_SECONDS", 0.05):
                result = await self.call(registry)
            self.assertTrue(result.is_error)
            self.assertIn("timed out", result.as_text())

    async def test_cancellation_propagates(self):
        async with example.parallel_registry() as registry:
            self.mode = "waiting"
            task = asyncio.create_task(self.call(registry))
            await asyncio.wait_for(self.started.wait(), timeout=2)
            task.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await task
        self.assertTrue(self.client.is_closed)

    async def test_missing_remote_tool(self):
        self.mode = "missing"
        # Transport task groups may wrap discovery errors in an ExceptionGroup.
        with self.assertRaises(Exception) as raised:
            async with example.parallel_registry():
                pass

        def contains_missing_tool(error):
            return (
                isinstance(error, RuntimeError)
                and "did not advertise search and fetch" in str(error)
            ) or any(contains_missing_tool(child) for child in getattr(error, "exceptions", ()))

        self.assertTrue(contains_missing_tool(raised.exception))
        self.assertTrue(self.client.is_closed)

    async def test_cli_output_arguments_and_exit_code(self):
        for mode, expected_status in (("normal", 0), ("tool_error", 1)):
            self.mode = mode
            output = io.StringIO()
            with redirect_stdout(output):
                status = await example.run(argparse.Namespace(
                    command="search", query="current Python release", session_id="conversation-123",
                ))
            self.assertEqual(status, expected_status)
            self.assertIn("https://www.python.org/", output.getvalue())
        payload = json.loads(self.requests[-1].content)
        self.assertEqual(payload["params"]["arguments"]["session_id"], "conversation-123")
        with redirect_stdout(io.StringIO()):
            await example.run(argparse.Namespace(
                command="fetch", urls=["https://www.python.org/"], session_id=None,
            ))
        args = json.loads(self.requests[-1].content)["params"]["arguments"]
        self.assertEqual(args, {"urls": ["https://www.python.org/"]})


if __name__ == "__main__":
    unittest.main()
