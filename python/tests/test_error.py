"""Tests for motosan_agent_tool.error."""

from motosan_agent_tool import ErrorKind, ToolError


class TestToolError:
    def test_missing_field(self) -> None:
        err = ToolError.missing_field("name")
        assert err.kind == ErrorKind.MISSING_FIELD
        assert "name" in str(err)

    def test_validation(self) -> None:
        err = ToolError.validation("bad value")
        assert err.kind == ErrorKind.VALIDATION
        assert "bad value" in str(err)

    def test_parse(self) -> None:
        err = ToolError.parse("invalid json")
        assert err.kind == ErrorKind.PARSE
        assert "invalid json" in str(err)

    def test_other(self) -> None:
        err = ToolError.other("something broke")
        assert err.kind == ErrorKind.OTHER
        assert str(err) == "something broke"

    def test_from_exception(self) -> None:
        original = ValueError("oops")
        err = ToolError.from_exception(original)
        assert err.kind == ErrorKind.OTHER
        assert "oops" in str(err)

    def test_is_exception(self) -> None:
        err = ToolError.other("boom")
        assert isinstance(err, Exception)

    def test_equality(self) -> None:
        a = ToolError.validation("x")
        b = ToolError.validation("x")
        assert a == b

    def test_inequality_different_kind(self) -> None:
        a = ToolError.validation("x")
        b = ToolError.parse("x")
        assert a != b

    def test_repr(self) -> None:
        err = ToolError.missing_field("id")
        r = repr(err)
        assert "ToolError" in r
        assert "MISSING_FIELD" in r
