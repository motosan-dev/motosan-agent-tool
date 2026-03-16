"""Core tool abstractions mirroring the Rust crate's tool module.

Classes:
    Tool        -- abstract base class for agent tools
    ToolDef     -- tool name + description + JSON Schema
    ToolResult  -- structured result returned by tool execution
    ToolContent -- tagged union: TextContent | JsonContent
    ToolContext -- execution context passed to every tool call
"""

from __future__ import annotations

import abc
from dataclasses import dataclass, field
from typing import Any

from .error import ToolError

# ---------------------------------------------------------------------------
# ToolContent  (tagged union)
# ---------------------------------------------------------------------------

_JSON = dict[str, Any] | list[Any] | str | int | float | bool | None
"""Alias for JSON-compatible Python values."""


@dataclass(frozen=True)
class TextContent:
    """Plain text content block."""

    text: str

    def to_dict(self) -> dict[str, Any]:
        return {"type": "text", "data": self.text}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> TextContent:
        return cls(text=data["data"])


@dataclass(frozen=True)
class JsonContent:
    """Structured JSON content block."""

    data: Any  # any JSON-serializable value

    def to_dict(self) -> dict[str, Any]:
        return {"type": "json", "data": self.data}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> JsonContent:
        return cls(data=data["data"])


ToolContent = TextContent | JsonContent
"""Tagged union of content blocks (mirrors Rust ``ToolContent`` enum)."""


def tool_content_from_dict(data: dict[str, Any]) -> ToolContent:
    """Deserialize a ``ToolContent`` from its dict representation."""
    type_tag = data.get("type")
    if type_tag == "text":
        return TextContent.from_dict(data)
    if type_tag == "json":
        return JsonContent.from_dict(data)
    raise ToolError.parse(f"unknown ToolContent type: {type_tag}")


# ---------------------------------------------------------------------------
# ToolDef
# ---------------------------------------------------------------------------


@dataclass
class ToolDef:
    """Definition of a tool, suitable for serialization to LLM APIs."""

    name: str
    description: str
    input_schema: dict[str, Any]

    # -- validation (mirrors Rust ``validate_input_schema``) ------------------

    def validate_input_schema(self) -> None:
        """Validate that ``input_schema`` is a well-formed JSON Schema object.

        Raises ``ToolError`` on failure.
        """
        schema = self.input_schema
        if not isinstance(schema, dict):
            raise ToolError.validation("input_schema must be a JSON object")

        if schema.get("type") != "object":
            raise ToolError.validation('input_schema.type must be "object"')

        properties = schema.get("properties")
        if not isinstance(properties, dict):
            raise ToolError.validation("input_schema.properties must be an object")

        required = schema.get("required")
        if required is not None:
            if not isinstance(required, list):
                raise ToolError.validation("input_schema.required must be an array")
            for entry in required:
                if not isinstance(entry, str):
                    raise ToolError.validation(
                        "input_schema.required entries must be strings"
                    )
                if entry not in properties:
                    raise ToolError.validation(
                        f"required field not in properties: {entry}"
                    )

    def validate_args(self, args: dict[str, Any]) -> None:
        """Validate *args* against the input schema.

        Checks required fields, types, and enum constraints.
        Raises ``ToolError`` on failure.
        """
        self.validate_input_schema()

        schema = self.input_schema
        properties: dict[str, Any] = schema["properties"]

        if not isinstance(args, dict):
            raise ToolError.validation("tool args must be a JSON object")

        # Required fields
        required = schema.get("required")
        if isinstance(required, list):
            for field_name in required:
                if field_name not in args:
                    raise ToolError.missing_field(field_name)

        # Type and enum checking
        _TYPE_CHECKERS: dict[str, type | tuple[type, ...]] = {
            "string": str,
            "number": (int, float),
            "integer": int,
            "boolean": bool,
            "object": dict,
            "array": list,
        }

        for key, value in args.items():
            spec = properties.get(key)
            if not isinstance(spec, dict):
                continue

            expected_type = spec.get("type")
            if isinstance(expected_type, str):
                # "null" needs special handling
                if expected_type == "null":
                    if value is not None:
                        raise ToolError.validation(
                            f"field {key} expected type {expected_type}"
                        )
                else:
                    checker = _TYPE_CHECKERS.get(expected_type)
                    if checker is not None:
                        # In Python ``bool`` is a subclass of ``int``; when we
                        # expect "integer" or "number" we must reject booleans.
                        if expected_type in ("integer", "number") and isinstance(
                            value, bool
                        ):
                            raise ToolError.validation(
                                f"field {key} expected type {expected_type}"
                            )
                        if not isinstance(value, checker):
                            raise ToolError.validation(
                                f"field {key} expected type {expected_type}"
                            )

            enum_values = spec.get("enum")
            if isinstance(enum_values, list):
                if value not in enum_values:
                    raise ToolError.validation(f"field {key} is not in enum")

    # -- serde helpers --------------------------------------------------------

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ToolDef:
        return cls(
            name=data["name"],
            description=data["description"],
            input_schema=data["input_schema"],
        )

    # -- equality (for tests) -------------------------------------------------

    def __eq__(self, other: Any) -> bool:
        if not isinstance(other, ToolDef):
            return NotImplemented
        return (
            self.name == other.name
            and self.description == other.description
            and self.input_schema == other.input_schema
        )


# ---------------------------------------------------------------------------
# ToolResult
# ---------------------------------------------------------------------------


@dataclass
class ToolResult:
    """Result returned by tool execution."""

    content: list[ToolContent] = field(default_factory=list)
    is_error: bool = False
    citation: str | None = None
    inject_to_context: bool = False
    duration_ms: int | None = None

    # -- convenience constructors ---------------------------------------------

    @classmethod
    def text(cls, text: str) -> ToolResult:
        """Successful text result."""
        return cls(content=[TextContent(text)])

    @classmethod
    def json(cls, value: Any) -> ToolResult:
        """Successful JSON result."""
        return cls(content=[JsonContent(value)])

    @classmethod
    def error(cls, message: str) -> ToolResult:
        """Error result."""
        return cls(content=[TextContent(message)], is_error=True)

    # -- builder methods (return *self* for chaining) -------------------------

    def with_citation(self, citation: str) -> ToolResult:
        self.citation = citation
        return self

    def with_inject(self, inject: bool) -> ToolResult:
        self.inject_to_context = inject
        return self

    def with_duration(self, ms: int) -> ToolResult:
        self.duration_ms = ms
        return self

    # -- accessors ------------------------------------------------------------

    def as_text(self) -> str | None:
        """Return the first text content, if any."""
        for c in self.content:
            if isinstance(c, TextContent):
                return c.text
        return None


# ---------------------------------------------------------------------------
# ToolContext
# ---------------------------------------------------------------------------


@dataclass
class ToolContext:
    """Execution context passed to every tool call."""

    caller_id: str
    platform: str
    extra: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def new(cls, caller_id: str, platform: str) -> ToolContext:
        return cls(caller_id=caller_id, platform=platform)

    def with_(self, key: str, value: Any) -> ToolContext:
        """Insert an extra field (builder pattern).

        Named ``with_`` to avoid shadowing the Python built-in ``with`` keyword.
        """
        self.extra[key] = value
        return self

    # -- typed accessors for extra --------------------------------------------

    def get_str(self, key: str) -> str | None:
        v = self.extra.get(key)
        return v if isinstance(v, str) else None

    def get_u64(self, key: str) -> int | None:
        v = self.extra.get(key)
        if isinstance(v, bool):
            return None
        return v if isinstance(v, int) and v >= 0 else None

    def get_bool(self, key: str) -> bool | None:
        v = self.extra.get(key)
        return v if isinstance(v, bool) else None


# ---------------------------------------------------------------------------
# Tool ABC
# ---------------------------------------------------------------------------


class Tool(abc.ABC):
    """Abstract base class for agent tools (mirrors Rust ``Tool`` trait)."""

    @abc.abstractmethod
    def def_(self) -> ToolDef:
        """Return the tool definition."""
        ...

    @abc.abstractmethod
    async def call(self, args: dict[str, Any], ctx: ToolContext) -> ToolResult:
        """Execute the tool."""
        ...
