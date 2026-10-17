# Web research with Parallel Search MCP

Use the Python `ToolRegistry` to search the web and retrieve page content with
[Parallel Search MCP](https://docs.parallel.ai/integrations/mcp/search-mcp).
The anonymous endpoint is free to use without an account or API key; search uses
Fast mode. Free access has lower rate limits.

This standalone example needs Python 3.10 or newer for the MCP SDK. From the
repository root, create an environment and install the local package and example
dependencies:

```bash
python3.10 -m venv .venv
. .venv/bin/activate
python -m pip install -e ./python -r python/examples/parallel_search/requirements.txt
```

Use your installed Python 3.10+ interpreter in place of `python3.10` if needed.
Search for sources, then fetch a public URL from the results:

```bash
python python/examples/parallel_search/parallel_search.py search "latest Python release highlights"
python python/examples/parallel_search/parallel_search.py fetch https://www.python.org/downloads/
```

The commands print MCP content, including source URLs and excerpts when returned
by the service. They dispatch tools directly without a model or agent loop.
To correlate related calls, supply the same conversation ID before the subcommand:

```bash
python python/examples/parallel_search/parallel_search.py --session-id 5f79bfea-0b2a-4fe0-8e70-29c3aef325e9 search "latest Python release highlights"
python python/examples/parallel_search/parallel_search.py --session-id 5f79bfea-0b2a-4fe0-8e70-29c3aef325e9 fetch https://www.python.org/downloads/
```

Choose a new ID for a new conversation and reuse it within that conversation.
Do not change it to bypass rate limits. MCP tool errors, including quota errors,
are printed and produce exit code 1. Connection or discovery failures raise an
exception. Requests have a 60-second timeout, and task cancellation propagates.

`parallel_registry()` connects over Streamable HTTP, discovers the server's input
schemas, and registers `parallel_web_search` and `parallel_web_fetch` in a new
registry. Keep calls inside its `async with` block so the session stays open.
The adapter preserves MCP text, structured content, other content blocks as JSON,
and the tool error flag. It sends no authorization header and does not read saved
credentials or API key environment variables. Only the example installs the MCP
dependencies; the package's dependencies and built-in tools are unchanged.

Run the example's offline checks in the same environment:

```bash
python -m unittest discover -s python/examples/parallel_search -p 'test_*.py' -v
```
