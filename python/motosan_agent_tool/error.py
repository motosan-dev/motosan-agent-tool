"""Error types mirroring the Rust crate's Error enum."""

from __future__ import annotations

from enum import Enum
from typing import Any


class ErrorKind(Enum):
    """Mirrors the Rust ``Error`` enum variants."""

    MISSING_FIELD = "missing_field"
    VALIDATION = "validation"
    PARSE = "parse"
    OTHER = "other"


class ToolError(Exception):
    """Unified error type for tool operations.

    Attributes:
        kind: The category of the error.
        message: Human-readable description.
    """

    def __init__(self, kind: ErrorKind, message: str) -> None:
        super().__init__(message)
        self.kind = kind
        self.message = message

    # -- convenience constructors (mirror Rust associated functions) ----------

    @classmethod
    def missing_field(cls, field: str) -> ToolError:
        """Required field is absent."""
        return cls(ErrorKind.MISSING_FIELD, f"missing required field: {field}")

    @classmethod
    def validation(cls, message: str) -> ToolError:
        """Schema or value validation failed."""
        return cls(ErrorKind.VALIDATION, f"validation failed: {message}")

    @classmethod
    def parse(cls, message: str) -> ToolError:
        """Parsing / deserialization error."""
        return cls(ErrorKind.PARSE, f"parse error: {message}")

    @classmethod
    def other(cls, message: str) -> ToolError:
        """Catch-all for unclassified errors."""
        return cls(ErrorKind.OTHER, message)

    # -- conversions ----------------------------------------------------------

    @classmethod
    def from_exception(cls, exc: Exception) -> ToolError:
        """Wrap an arbitrary exception as ``ErrorKind.OTHER``."""
        return cls(ErrorKind.OTHER, str(exc))

    # -- dunder ---------------------------------------------------------------

    def __repr__(self) -> str:
        return f"ToolError(kind={self.kind!r}, message={self.message!r})"

    def __eq__(self, other: Any) -> bool:
        if not isinstance(other, ToolError):
            return NotImplemented
        return self.kind == other.kind and self.message == other.message
