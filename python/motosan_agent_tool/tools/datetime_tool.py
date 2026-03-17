"""DatetimeTool -- built-in tool for date/time operations.

Provides three functions dispatched via the ``function`` argument:

- ``get_current_datetime`` -- current datetime with rich output
- ``date_add``            -- add an offset to a base date
- ``date_diff``           -- difference between two dates
"""

from __future__ import annotations

import calendar
import re
from datetime import date, datetime, time, timedelta
from typing import Any

from ..error import ToolError
from ..tool import Tool, ToolContext, ToolDef, ToolResult

try:
    from zoneinfo import ZoneInfo  # Python 3.9+
except ImportError:  # pragma: no cover
    from backports.zoneinfo import ZoneInfo  # type: ignore[no-redef]

# ---------------------------------------------------------------------------
# Offset parser (e.g. "+1d", "-2w", "+1M", "next monday")
# ---------------------------------------------------------------------------

_OFFSET_RE = re.compile(r"^([+-])(\d+)([dwM])$")

_WEEKDAYS = {
    "monday": 0, "mon": 0,
    "tuesday": 1, "tue": 1,
    "wednesday": 2, "wed": 2,
    "thursday": 3, "thu": 3,
    "friday": 4, "fri": 4,
    "saturday": 5, "sat": 5,
    "sunday": 6, "sun": 6,
}

_WEEKDAY_NAMES = [
    "Monday", "Tuesday", "Wednesday", "Thursday",
    "Friday", "Saturday", "Sunday",
]


def _parse_offset(base: date, offset: str) -> date:
    """Parse an offset expression and apply it to *base*.

    Supported forms:
    - ``+Nd`` / ``-Nd`` -- add/subtract N days
    - ``+Nw`` / ``-Nw`` -- add/subtract N weeks
    - ``+NM`` / ``-NM`` -- add/subtract N months (clamp to end of month)
    - ``next monday`` etc. -- next occurrence of a weekday (always future)
    """
    trimmed = offset.strip()

    # Handle "next <weekday>"
    if trimmed.startswith("next "):
        day_name = trimmed[5:].strip().lower()
        target = _WEEKDAYS.get(day_name)
        if target is None:
            raise ValueError(f"Unknown weekday: {day_name}")
        d = base + timedelta(days=1)
        while d.weekday() != target:
            d += timedelta(days=1)
        return d

    # Handle "+Nd", "-Nd", "+Nw", "+NM" etc.
    match = _OFFSET_RE.match(trimmed)
    if match is None:
        raise ValueError(
            f'Invalid offset "{offset}": expected format like +1d, -7d, +2w, +1M, or "next monday"'
        )

    sign_str, num_str, unit = match.groups()
    n = int(num_str)
    sign = 1 if sign_str == "+" else -1

    if unit == "d":
        return base + timedelta(days=sign * n)
    if unit == "w":
        return base + timedelta(weeks=sign * n)
    if unit == "M":
        return _add_months(base, sign * n)

    raise ValueError(f'Invalid offset unit "{unit}": expected d (days), w (weeks), or M (months)')


def _add_months(base: date, months: int) -> date:
    """Add *months* to *base*, clamping day to end of target month."""
    total_months = base.year * 12 + (base.month - 1) + months
    target_year = total_months // 12
    target_month = total_months % 12 + 1

    max_day = calendar.monthrange(target_year, target_month)[1]
    target_day = min(base.day, max_day)
    return date(target_year, target_month, target_day)


# ---------------------------------------------------------------------------
# Formatting helpers
# ---------------------------------------------------------------------------


def _ordinal_suffix(day: int) -> str:
    if day in (1, 21, 31):
        return "st"
    if day in (2, 22):
        return "nd"
    if day in (3, 23):
        return "rd"
    return "th"


def _format_human_datetime(dt: datetime) -> str:
    """Format like: Tuesday, March 17th, 2026 -- 9:25 PM"""
    weekday = _WEEKDAY_NAMES[dt.weekday()]
    month = dt.strftime("%B")
    day = dt.day
    year = dt.year
    hour = dt.strftime("%-I")
    minute = dt.strftime("%M")
    ampm = dt.strftime("%p")
    suffix = _ordinal_suffix(day)
    return f"{weekday}, {month} {day}{suffix}, {year} \u2014 {hour}:{minute} {ampm}"


def _format_human_date(d: date) -> str:
    """Format like: Wednesday, March 18th, 2026"""
    weekday = _WEEKDAY_NAMES[d.weekday()]
    # Use a datetime to get month name
    month = date(d.year, d.month, 1).strftime("%B")
    day = d.day
    year = d.year
    suffix = _ordinal_suffix(day)
    return f"{weekday}, {month} {day}{suffix}, {year}"


def _approximate_months(from_date: date, to_date: date) -> int:
    """Approximate whole months between two dates (matches Rust logic)."""
    year_diff = to_date.year - from_date.year
    month_diff = to_date.month - from_date.month
    total = year_diff * 12 + month_diff
    if total > 0 and to_date.day < from_date.day:
        total -= 1
    elif total < 0 and to_date.day > from_date.day:
        total += 1
    return total


def _format_human_diff(days: int) -> str:
    """Human-readable diff string (matches Rust logic)."""
    abs_days = abs(days)
    prefix = "minus " if days < 0 else ""

    if abs_days == 0:
        return "0 days"

    weeks = abs_days // 7
    remaining_days = abs_days % 7

    if abs_days < 7:
        plural = "" if abs_days == 1 else "s"
        return f"{prefix}{abs_days} day{plural}"
    if remaining_days == 0:
        plural = "" if weeks == 1 else "s"
        return f"{prefix}{weeks} week{plural}"
    months = abs_days // 30
    if months >= 1 and abs_days % 30 == 0:
        plural = "" if months == 1 else "s"
        return f"{prefix}{months} month{plural}"
    return f"{prefix}{abs_days} days"


# ---------------------------------------------------------------------------
# DatetimeTool
# ---------------------------------------------------------------------------

_INPUT_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "function": {
            "type": "string",
            "enum": ["get_current_datetime", "date_add", "date_diff"],
            "description": "The datetime function to call",
        },
        "timezone": {
            "type": "string",
            "description": (
                'IANA timezone (e.g. "Asia/Taipei", "US/Eastern"). '
                "Defaults to UTC."
            ),
        },
        "date": {
            "type": "string",
            "description": "Base date for date_add in YYYY-MM-DD format",
        },
        "offset": {
            "type": "string",
            "description": (
                'Offset for date_add: "+1d", "-7d", "+2w", "+1M", '
                '"next monday", etc.'
            ),
        },
        "from": {
            "type": "string",
            "description": "Start date for date_diff in YYYY-MM-DD format",
        },
        "to": {
            "type": "string",
            "description": "End date for date_diff in YYYY-MM-DD format",
        },
    },
    "required": ["function"],
}


class DatetimeTool(Tool):
    """Built-in tool for date/time operations.

    Functions:
        get_current_datetime -- current time in a given timezone
        date_add             -- add an offset to a base date
        date_diff            -- difference between two dates
    """

    def def_(self) -> ToolDef:
        return ToolDef(
            name="datetime",
            description=(
                "Date and time utilities. Supports getting the current datetime, "
                "adding offsets to dates, and calculating differences between dates."
            ),
            input_schema=_INPUT_SCHEMA,
        )

    async def call(self, args: dict[str, Any], ctx: ToolContext) -> ToolResult:  # noqa: ARG002
        try:
            self.def_().validate_args(args)
        except ToolError as exc:
            return ToolResult.error(str(exc))

        fn = args["function"]
        if fn == "get_current_datetime":
            return self._get_current_datetime(args)
        if fn == "date_add":
            return self._date_add(args)
        if fn == "date_diff":
            return self._date_diff(args)

        return ToolResult.error(
            f"Unknown function: {fn}. Expected one of: get_current_datetime, date_add, date_diff"
        )

    # -- function implementations ---------------------------------------------

    @staticmethod
    def _get_current_datetime(args: dict[str, Any]) -> ToolResult:
        tz_name = args.get("timezone", "UTC")
        try:
            tz = ZoneInfo(tz_name)
        except (KeyError, Exception):
            return ToolResult.error(f"Unknown timezone: {tz_name}")

        now = datetime.now(tz=tz)
        iso = now.isoformat()
        d = now.strftime("%Y-%m-%d")
        t = now.strftime("%H:%M")
        weekday = _WEEKDAY_NAMES[now.weekday()]
        human = _format_human_datetime(now)

        return ToolResult.json({
            "iso": iso,
            "date": d,
            "time": t,
            "weekday": weekday,
            "human": human,
        })

    @staticmethod
    def _date_add(args: dict[str, Any]) -> ToolResult:
        date_str = args.get("date")
        offset_str = args.get("offset")
        if not date_str:
            return ToolResult.error('date_add requires a "date" field (YYYY-MM-DD)')
        if not offset_str:
            return ToolResult.error('date_add requires an "offset" field')

        try:
            base = date.fromisoformat(date_str)
        except ValueError as exc:
            return ToolResult.error(f'Invalid date "{date_str}": {exc}')

        try:
            result_date = _parse_offset(base, offset_str)
        except ValueError as exc:
            return ToolResult.error(str(exc))

        tz_name = args.get("timezone", "UTC")
        try:
            tz = ZoneInfo(tz_name)
        except (KeyError, Exception):
            return ToolResult.error(f"Unknown timezone: {tz_name}")

        dt = datetime.combine(result_date, time(0, 0, 0), tzinfo=tz)
        iso = dt.isoformat()
        d = dt.strftime("%Y-%m-%d")
        weekday = _WEEKDAY_NAMES[dt.weekday()]
        human = _format_human_date(result_date)

        return ToolResult.json({
            "iso": iso,
            "date": d,
            "weekday": weekday,
            "human": human,
        })

    @staticmethod
    def _date_diff(args: dict[str, Any]) -> ToolResult:
        from_str = args.get("from")
        to_str = args.get("to")
        if not from_str:
            return ToolResult.error('date_diff requires a "from" field (YYYY-MM-DD)')
        if not to_str:
            return ToolResult.error('date_diff requires a "to" field (YYYY-MM-DD)')

        try:
            from_date = date.fromisoformat(from_str)
        except ValueError as exc:
            return ToolResult.error(f'Invalid from date "{from_str}": {exc}')

        try:
            to_date = date.fromisoformat(to_str)
        except ValueError as exc:
            return ToolResult.error(f'Invalid to date "{to_str}": {exc}')

        days = (to_date - from_date).days
        abs_days = abs(days)
        weeks = abs_days // 7
        months = _approximate_months(from_date, to_date)
        human = _format_human_diff(days)

        return ToolResult.json({
            "days": days,
            "weeks": weeks,
            "months": months,
            "human": human,
        })
