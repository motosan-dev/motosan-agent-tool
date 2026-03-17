import { type Tool, ToolDef, ToolResult, type ToolContext } from "../tool.js";
import { ToolError } from "../error.js";

// ---------------------------------------------------------------------------
// Input types (flat — no nested "args" object)
// ---------------------------------------------------------------------------

interface DatetimeInput {
  function: string;
  timezone?: string;
  date?: string;
  offset?: string;
  from?: string;
  to?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function resolveTimezone(tz?: string): string {
  const name = tz ?? "UTC";
  // Validate by trying to use it
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: name });
  } catch {
    throw ToolError.validation(`Unknown timezone: ${name}`);
  }
  return name;
}

function parseDate(dateStr: string): Date {
  // Only accept YYYY-MM-DD format
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateStr);
  if (!m) {
    throw ToolError.validation(`Invalid date "${dateStr}": expected YYYY-MM-DD format`);
  }
  const year = parseInt(m[1], 10);
  const month = parseInt(m[2], 10);
  const day = parseInt(m[3], 10);
  const d = new Date(Date.UTC(year, month - 1, day));
  if (isNaN(d.getTime()) || d.getUTCFullYear() !== year || d.getUTCMonth() !== month - 1 || d.getUTCDate() !== day) {
    throw ToolError.validation(`Invalid date "${dateStr}"`);
  }
  return d;
}

function daysInMonth(year: number, month: number): number {
  // month is 1-based
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

function addMonths(base: Date, months: number): Date {
  const baseYear = base.getUTCFullYear();
  const baseMonth = base.getUTCMonth(); // 0-based
  const baseDay = base.getUTCDate();

  const totalMonths = baseYear * 12 + baseMonth + months;
  const targetYear = Math.floor(totalMonths / 12);
  const targetMonth = totalMonths % 12; // 0-based
  const maxDay = daysInMonth(targetYear, targetMonth + 1);
  const targetDay = Math.min(baseDay, maxDay);

  return new Date(Date.UTC(targetYear, targetMonth, targetDay));
}

const WEEKDAY_MAP: Record<string, number> = {
  sunday: 0, sun: 0,
  monday: 1, mon: 1,
  tuesday: 2, tue: 2,
  wednesday: 3, wed: 3,
  thursday: 4, thu: 4,
  friday: 5, fri: 5,
  saturday: 6, sat: 6,
};

const WEEKDAY_NAMES = [
  "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
];

const MONTH_NAMES = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

function parseOffset(base: Date, offset: string): Date {
  const trimmed = offset.trim();

  // Handle "next <weekday>"
  if (trimmed.startsWith("next ")) {
    const dayName = trimmed.slice(5).trim().toLowerCase();
    const targetDay = WEEKDAY_MAP[dayName];
    if (targetDay === undefined) {
      throw ToolError.validation(`Unknown weekday: ${dayName}`);
    }
    const result = new Date(base.getTime());
    // Advance at least 1 day, then find the target weekday
    result.setUTCDate(result.getUTCDate() + 1);
    while (result.getUTCDay() !== targetDay) {
      result.setUTCDate(result.getUTCDate() + 1);
    }
    return result;
  }

  // Handle "+Nd", "-Nd", "+Nw", "+NM"
  const m = /^([+-])(\d+)([dwM])$/.exec(trimmed);
  if (!m) {
    throw ToolError.validation(
      `Invalid offset "${offset}": expected format like +1d, -7d, +2w, +1M, or "next monday"`,
    );
  }

  const sign = m[1] === "+" ? 1 : -1;
  const n = parseInt(m[2], 10);
  const unit = m[3];

  switch (unit) {
    case "d": {
      const result = new Date(base.getTime());
      result.setUTCDate(result.getUTCDate() + sign * n);
      return result;
    }
    case "w": {
      const result = new Date(base.getTime());
      result.setUTCDate(result.getUTCDate() + sign * n * 7);
      return result;
    }
    case "M":
      return addMonths(base, sign * n);
    default:
      throw ToolError.validation(
        `Invalid offset unit "${unit}": expected d (days), w (weeks), or M (months)`,
      );
  }
}

function ordinalSuffix(day: number): string {
  switch (day) {
    case 1: case 21: case 31: return "st";
    case 2: case 22: return "nd";
    case 3: case 23: return "rd";
    default: return "th";
  }
}

function formatHumanDatetime(date: Date, tz: string): string {
  const parts = getDatePartsInTz(date, tz);
  const suffix = ordinalSuffix(parts.day);
  const hour12 = parts.hour % 12 || 12;
  const ampm = parts.hour >= 12 ? "PM" : "AM";
  const minute = String(parts.minute).padStart(2, "0");
  return `${parts.weekday}, ${parts.month} ${parts.day}${suffix}, ${parts.year} \u2014 ${hour12}:${minute} ${ampm}`;
}

function formatHumanDate(date: Date, tz: string): string {
  const parts = getDatePartsInTz(date, tz);
  const suffix = ordinalSuffix(parts.day);
  return `${parts.weekday}, ${parts.month} ${parts.day}${suffix}, ${parts.year}`;
}

interface DateParts {
  year: number;
  month: string;
  day: number;
  weekday: string;
  hour: number;
  minute: number;
  second: number;
  dateStr: string; // YYYY-MM-DD
  timeStr: string; // HH:MM
}

function getDatePartsInTz(date: Date, tz: string): DateParts {
  const formatter = new Intl.DateTimeFormat("en-US", {
    timeZone: tz,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    weekday: "long",
    hour12: false,
  });

  const parts = formatter.formatToParts(date);
  const get = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((p) => p.type === type)?.value ?? "";

  const year = parseInt(get("year"), 10);
  const monthNum = parseInt(get("month"), 10);
  const day = parseInt(get("day"), 10);
  let hour = parseInt(get("hour"), 10);
  // Intl may return 24 for midnight in hour12:false mode
  if (hour === 24) hour = 0;
  const minute = parseInt(get("minute"), 10);
  const second = parseInt(get("second"), 10);
  const weekday = get("weekday");
  const month = MONTH_NAMES[monthNum - 1];

  const dateStr = `${String(year).padStart(4, "0")}-${String(monthNum).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
  const timeStr = `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;

  return { year, month, day, weekday, hour, minute, second, dateStr, timeStr };
}

function getIsoInTz(date: Date, tz: string): string {
  const parts = getDatePartsInTz(date, tz);
  // Get timezone offset
  const utcMs = date.getTime();
  // Create a date string in the target timezone and parse it to find the offset
  const tzDate = new Date(
    new Intl.DateTimeFormat("en-CA", {
      timeZone: tz,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }).format(date).replace(/, /, "T"),
  );

  // Calculate offset in minutes
  const localMs = tzDate.getTime();
  const offsetMin = Math.round((localMs - utcMs) / 60000);

  // Better approach: use the Intl API to get the offset
  const offsetStr = getTimezoneOffset(date, tz);

  return `${parts.dateStr}T${parts.timeStr}:${String(parts.second).padStart(2, "0")}${offsetStr}`;
}

function getTimezoneOffset(date: Date, tz: string): string {
  // Get offset by comparing UTC representation with local representation
  const utcFormatter = new Intl.DateTimeFormat("en-CA", {
    timeZone: "UTC",
    year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", second: "2-digit",
    hour12: false,
  });
  const tzFormatter = new Intl.DateTimeFormat("en-CA", {
    timeZone: tz,
    year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", second: "2-digit",
    hour12: false,
  });

  const utcStr = utcFormatter.format(date).replace(/, /, "T");
  const tzStr = tzFormatter.format(date).replace(/, /, "T");

  // Parse both as UTC to find the difference
  const utcParsed = new Date(utcStr + "Z");
  const tzParsed = new Date(tzStr + "Z");

  const diffMs = tzParsed.getTime() - utcParsed.getTime();
  const diffMin = Math.round(diffMs / 60000);

  if (diffMin === 0) return "+00:00";

  const sign = diffMin > 0 ? "+" : "-";
  const absDiff = Math.abs(diffMin);
  const hours = Math.floor(absDiff / 60);
  const minutes = absDiff % 60;
  return `${sign}${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

function approximateMonths(from: Date, to: Date): number {
  const fromYear = from.getUTCFullYear();
  const fromMonth = from.getUTCMonth();
  const fromDay = from.getUTCDate();
  const toYear = to.getUTCFullYear();
  const toMonth = to.getUTCMonth();
  const toDay = to.getUTCDate();

  const total = (toYear - fromYear) * 12 + (toMonth - fromMonth);
  if (total > 0 && toDay < fromDay) {
    return total - 1;
  } else if (total < 0 && toDay > fromDay) {
    return total + 1;
  }
  return total;
}

function formatHumanDiff(days: number): string {
  const absDays = Math.abs(days);
  const prefix = days < 0 ? "minus " : "";

  if (absDays === 0) return "0 days";

  const weeks = Math.floor(absDays / 7);
  const remainingDays = absDays % 7;
  const months = Math.floor(absDays / 30);

  if (absDays < 7) {
    return `${prefix}${absDays} day${absDays === 1 ? "" : "s"}`;
  } else if (remainingDays === 0) {
    return `${prefix}${weeks} week${weeks === 1 ? "" : "s"}`;
  } else if (months >= 1 && absDays % 30 === 0) {
    return `${prefix}${months} month${months === 1 ? "" : "s"}`;
  } else {
    return `${prefix}${absDays} days`;
  }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

function handleGetCurrentDatetime(input: DatetimeInput): ToolResult {
  let tz: string;
  try {
    tz = resolveTimezone(input.timezone);
  } catch (err) {
    if (err instanceof ToolError) return ToolResult.error(err.message);
    throw err;
  }

  const now = new Date();
  const iso = getIsoInTz(now, tz);
  const parts = getDatePartsInTz(now, tz);
  const human = formatHumanDatetime(now, tz);

  return ToolResult.json({
    iso,
    date: parts.dateStr,
    time: parts.timeStr,
    weekday: parts.weekday,
    human,
  });
}

function handleDateAdd(input: DatetimeInput): ToolResult {
  let tz: string;
  try {
    tz = resolveTimezone(input.timezone);
  } catch (err) {
    if (err instanceof ToolError) return ToolResult.error(err.message);
    throw err;
  }

  if (!input.date) {
    return ToolResult.error('date_add requires a "date" field (YYYY-MM-DD)');
  }
  if (!input.offset) {
    return ToolResult.error('date_add requires an "offset" field');
  }

  let base: Date;
  try {
    base = parseDate(input.date);
  } catch (err) {
    if (err instanceof ToolError) return ToolResult.error(err.message);
    throw err;
  }

  let resultDate: Date;
  try {
    resultDate = parseOffset(base, input.offset);
  } catch (err) {
    if (err instanceof ToolError) return ToolResult.error(err.message);
    throw err;
  }

  const iso = getIsoInTz(resultDate, tz);
  const parts = getDatePartsInTz(resultDate, tz);
  const human = formatHumanDate(resultDate, tz);

  return ToolResult.json({
    iso,
    date: parts.dateStr,
    weekday: parts.weekday,
    human,
  });
}

function handleDateDiff(input: DatetimeInput): ToolResult {
  if (!input.from) {
    return ToolResult.error('date_diff requires a "from" field (YYYY-MM-DD)');
  }
  if (!input.to) {
    return ToolResult.error('date_diff requires a "to" field (YYYY-MM-DD)');
  }

  let from: Date;
  let to: Date;
  try {
    from = parseDate(input.from);
    to = parseDate(input.to);
  } catch (err) {
    if (err instanceof ToolError) return ToolResult.error(err.message);
    throw err;
  }

  const diffMs = to.getTime() - from.getTime();
  const days = Math.round(diffMs / (1000 * 60 * 60 * 24));
  const absDays = Math.abs(days);
  const weeks = Math.floor(absDays / 7);
  const months = approximateMonths(from, to);
  const human = formatHumanDiff(days);

  return ToolResult.json({
    days,
    weeks,
    months,
    human,
  });
}

// ---------------------------------------------------------------------------
// Input schema (flat — matches Rust)
// ---------------------------------------------------------------------------

const INPUT_SCHEMA: Record<string, unknown> = {
  type: "object",
  properties: {
    function: {
      type: "string",
      enum: ["get_current_datetime", "date_add", "date_diff"],
      description: "The datetime function to call",
    },
    timezone: {
      type: "string",
      description: 'IANA timezone (e.g. "Asia/Taipei", "US/Eastern"). Defaults to UTC.',
    },
    date: {
      type: "string",
      description: "Base date for date_add in YYYY-MM-DD format",
    },
    offset: {
      type: "string",
      description: 'Offset for date_add: "+1d", "-7d", "+2w", "+1M", "next monday", etc.',
    },
    from: {
      type: "string",
      description: "Start date for date_diff in YYYY-MM-DD format",
    },
    to: {
      type: "string",
      description: "End date for date_diff in YYYY-MM-DD format",
    },
  },
  required: ["function"],
};

// ---------------------------------------------------------------------------
// DatetimeTool
// ---------------------------------------------------------------------------

export class DatetimeTool implements Tool {
  def(): ToolDef {
    return new ToolDef(
      "datetime",
      "Date and time utilities. Supports getting the current datetime, " +
        "adding offsets to dates, and calculating differences between dates.",
      INPUT_SCHEMA,
    );
  }

  async call(args: unknown, _ctx: ToolContext): Promise<ToolResult> {
    const toolDef = this.def();
    toolDef.validateArgs(args);

    const input = args as DatetimeInput;

    switch (input.function) {
      case "get_current_datetime":
        return handleGetCurrentDatetime(input);
      case "date_add":
        return handleDateAdd(input);
      case "date_diff":
        return handleDateDiff(input);
      default:
        return ToolResult.error(
          `Unknown function: ${input.function}. Expected one of: get_current_datetime, date_add, date_diff`,
        );
    }
  }
}
