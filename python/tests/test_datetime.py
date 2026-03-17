"""Tests for DatetimeTool built-in (matching Rust API)."""

from __future__ import annotations

from datetime import datetime

import pytest

from motosan_agent_tool import DatetimeTool, ToolContext


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _ctx() -> ToolContext:
    return ToolContext.new("test", "unit")


def _tool() -> DatetimeTool:
    return DatetimeTool()


# ---------------------------------------------------------------------------
# ToolDef
# ---------------------------------------------------------------------------


class TestDatetimeToolDef:
    def test_name(self) -> None:
        d = _tool().def_()
        assert d.name == "datetime"

    def test_has_function_enum(self) -> None:
        d = _tool().def_()
        fn_prop = d.input_schema["properties"]["function"]
        assert set(fn_prop["enum"]) == {
            "get_current_datetime",
            "date_add",
            "date_diff",
        }

    def test_has_correct_fields(self) -> None:
        d = _tool().def_()
        props = d.input_schema["properties"]
        assert "function" in props
        assert "timezone" in props
        assert "date" in props
        assert "offset" in props
        assert "from" in props
        assert "to" in props
        assert d.input_schema["required"] == ["function"]

    def test_validate_input_schema(self) -> None:
        _tool().def_().validate_input_schema()


# ---------------------------------------------------------------------------
# get_current_datetime
# ---------------------------------------------------------------------------


class TestGetCurrentDatetime:
    @pytest.mark.asyncio
    async def test_returns_all_fields(self) -> None:
        result = await _tool().call(
            {"function": "get_current_datetime"}, _ctx()
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert "iso" in data
        assert "date" in data
        assert "time" in data
        assert "weekday" in data
        assert "human" in data
        # ISO string should be parseable and non-empty
        assert data["iso"]
        datetime.fromisoformat(data["iso"])

    @pytest.mark.asyncio
    async def test_with_timezone_asia_taipei(self) -> None:
        result = await _tool().call(
            {"function": "get_current_datetime", "timezone": "Asia/Taipei"},
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        # Taipei is UTC+8, so the offset should contain +08:00
        assert "+08:00" in data["iso"]

    @pytest.mark.asyncio
    async def test_invalid_timezone(self) -> None:
        result = await _tool().call(
            {"function": "get_current_datetime", "timezone": "Foo/Bar"},
            _ctx(),
        )
        assert result.is_error


# ---------------------------------------------------------------------------
# date_add
# ---------------------------------------------------------------------------


class TestDateAdd:
    @pytest.mark.asyncio
    async def test_add_one_day(self) -> None:
        result = await _tool().call(
            {
                "function": "date_add",
                "date": "2026-03-17",
                "offset": "+1d",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["date"] == "2026-03-18"
        assert data["weekday"] == "Wednesday"

    @pytest.mark.asyncio
    async def test_add_two_weeks(self) -> None:
        result = await _tool().call(
            {
                "function": "date_add",
                "date": "2026-03-17",
                "offset": "+2w",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["date"] == "2026-03-31"

    @pytest.mark.asyncio
    async def test_subtract_days(self) -> None:
        result = await _tool().call(
            {
                "function": "date_add",
                "date": "2026-03-17",
                "offset": "-7d",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["date"] == "2026-03-10"

    @pytest.mark.asyncio
    async def test_next_monday(self) -> None:
        # 2026-03-17 is a Tuesday
        result = await _tool().call(
            {
                "function": "date_add",
                "date": "2026-03-17",
                "offset": "next monday",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["date"] == "2026-03-23"
        assert data["weekday"] == "Monday"

    @pytest.mark.asyncio
    async def test_add_one_month_clamped(self) -> None:
        # Jan 31 + 1M = Feb 28 (clamped)
        result = await _tool().call(
            {
                "function": "date_add",
                "date": "2026-01-31",
                "offset": "+1M",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["date"] == "2026-02-28"

    @pytest.mark.asyncio
    async def test_output_has_iso_and_human(self) -> None:
        result = await _tool().call(
            {
                "function": "date_add",
                "date": "2026-03-17",
                "offset": "+1d",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert "iso" in data
        assert "human" in data
        assert "weekday" in data

    @pytest.mark.asyncio
    async def test_missing_date(self) -> None:
        result = await _tool().call(
            {"function": "date_add", "offset": "+1d"}, _ctx()
        )
        assert result.is_error

    @pytest.mark.asyncio
    async def test_missing_offset(self) -> None:
        result = await _tool().call(
            {"function": "date_add", "date": "2026-03-17"}, _ctx()
        )
        assert result.is_error

    @pytest.mark.asyncio
    async def test_invalid_offset(self) -> None:
        result = await _tool().call(
            {
                "function": "date_add",
                "date": "2026-03-17",
                "offset": "garbage",
            },
            _ctx(),
        )
        assert result.is_error


# ---------------------------------------------------------------------------
# date_diff
# ---------------------------------------------------------------------------


class TestDateDiff:
    @pytest.mark.asyncio
    async def test_two_weeks(self) -> None:
        result = await _tool().call(
            {
                "function": "date_diff",
                "from": "2026-03-17",
                "to": "2026-03-31",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["days"] == 14
        assert data["weeks"] == 2
        assert data["months"] == 0
        assert data["human"] == "2 weeks"

    @pytest.mark.asyncio
    async def test_negative_diff(self) -> None:
        result = await _tool().call(
            {
                "function": "date_diff",
                "from": "2026-03-17",
                "to": "2026-03-01",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["days"] == -16

    @pytest.mark.asyncio
    async def test_same_date(self) -> None:
        result = await _tool().call(
            {
                "function": "date_diff",
                "from": "2026-03-17",
                "to": "2026-03-17",
            },
            _ctx(),
        )
        assert not result.is_error
        data = result.content[0].data  # type: ignore[union-attr]
        assert data["days"] == 0
        assert data["human"] == "0 days"

    @pytest.mark.asyncio
    async def test_missing_from(self) -> None:
        result = await _tool().call(
            {"function": "date_diff", "to": "2026-03-17"}, _ctx()
        )
        assert result.is_error

    @pytest.mark.asyncio
    async def test_missing_to(self) -> None:
        result = await _tool().call(
            {"function": "date_diff", "from": "2026-03-17"}, _ctx()
        )
        assert result.is_error


# ---------------------------------------------------------------------------
# Unknown function
# ---------------------------------------------------------------------------


class TestUnknownFunction:
    @pytest.mark.asyncio
    async def test_unknown_function_returns_error(self) -> None:
        result = await _tool().call(
            {"function": "not_a_function"}, _ctx()
        )
        assert result.is_error
