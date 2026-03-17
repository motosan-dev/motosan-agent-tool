import { describe, it, expect } from "vitest";
import { DatetimeTool } from "../src/tools/datetime.js";
import { ToolContext } from "../src/tool.js";

const tool = new DatetimeTool();
const ctx = ToolContext.create("test", "unit");

// Helper to extract JSON data from a ToolResult
function jsonData(result: { content: { type: string; data: unknown }[] }): Record<string, unknown> {
  const json = result.content.find((c) => c.type === "json");
  return json?.data as Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// def()
// ---------------------------------------------------------------------------

describe("DatetimeTool.def()", () => {
  it("has correct name and schema", () => {
    const d = tool.def();
    expect(d.name).toBe("datetime");
    expect(d.description).toBeTruthy();

    const schema = d.inputSchema;
    expect(schema["type"]).toBe("object");
    expect(schema["properties"]).toBeTruthy();

    const props = schema["properties"] as Record<string, unknown>;
    expect(props["function"]).toBeTruthy();
    expect(props["timezone"]).toBeTruthy();
    expect(props["date"]).toBeTruthy();
    expect(props["offset"]).toBeTruthy();
    expect(props["from"]).toBeTruthy();
    expect(props["to"]).toBeTruthy();
    expect(schema["required"]).toEqual(["function"]);
  });

  it("passes schema validation", () => {
    const d = tool.def();
    expect(() => d.validateInputSchema()).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// get_current_datetime
// ---------------------------------------------------------------------------

describe("get_current_datetime", () => {
  it("returns iso, date, time, weekday, human fields", async () => {
    const result = await tool.call({ function: "get_current_datetime" }, ctx);
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(typeof data.iso).toBe("string");
    expect(typeof data.date).toBe("string");
    expect(typeof data.time).toBe("string");
    expect(typeof data.weekday).toBe("string");
    expect(typeof data.human).toBe("string");
    // ISO string should not be empty
    expect((data.iso as string).length).toBeGreaterThan(0);
    // date should be YYYY-MM-DD
    expect(data.date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    // time should be HH:MM
    expect(data.time).toMatch(/^\d{2}:\d{2}$/);
  });

  it("respects timezone parameter (Asia/Taipei = UTC+8)", async () => {
    const result = await tool.call(
      { function: "get_current_datetime", timezone: "Asia/Taipei" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    const iso = data.iso as string;
    expect(iso).toContain("+08:00");
  });
});

// ---------------------------------------------------------------------------
// date_add
// ---------------------------------------------------------------------------

describe("date_add", () => {
  it("adds 1 day to 2026-03-17", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17", offset: "+1d" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.date).toBe("2026-03-18");
    expect(data.weekday).toBe("Wednesday");
  });

  it("adds 2 weeks to 2026-03-17", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17", offset: "+2w" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.date).toBe("2026-03-31");
  });

  it("handles next monday from 2026-03-17 (Tuesday)", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17", offset: "next monday" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.date).toBe("2026-03-23");
    expect(data.weekday).toBe("Monday");
  });

  it("adds 1 month with end-of-month clamping (Jan 31 + 1M = Feb 28)", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-01-31", offset: "+1M" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.date).toBe("2026-02-28");
  });

  it("subtracts 7 days", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17", offset: "-7d" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.date).toBe("2026-03-10");
  });

  it("returns iso, date, weekday, human fields", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17", offset: "+1d" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(typeof data.iso).toBe("string");
    expect(typeof data.date).toBe("string");
    expect(typeof data.weekday).toBe("string");
    expect(typeof data.human).toBe("string");
  });

  it("returns error for missing date", async () => {
    const result = await tool.call(
      { function: "date_add", offset: "+1d" },
      ctx,
    );
    expect(result.isError).toBe(true);
  });

  it("returns error for missing offset", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17" },
      ctx,
    );
    expect(result.isError).toBe(true);
  });

  it("returns error for invalid offset", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17", offset: "garbage" },
      ctx,
    );
    expect(result.isError).toBe(true);
  });

  it("returns error for invalid offset unit", async () => {
    const result = await tool.call(
      { function: "date_add", date: "2026-03-17", offset: "+1x" },
      ctx,
    );
    expect(result.isError).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// date_diff
// ---------------------------------------------------------------------------

describe("date_diff", () => {
  it("computes 2 weeks difference", async () => {
    const result = await tool.call(
      { function: "date_diff", from: "2026-03-17", to: "2026-03-31" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.days).toBe(14);
    expect(data.weeks).toBe(2);
    expect(data.months).toBe(0);
    expect(data.human).toBe("2 weeks");
  });

  it("computes negative difference", async () => {
    const result = await tool.call(
      { function: "date_diff", from: "2026-03-31", to: "2026-03-17" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.days).toBe(-14);
    expect((data.human as string)).toContain("minus");
  });

  it("returns 0 days for same dates", async () => {
    const result = await tool.call(
      { function: "date_diff", from: "2026-03-17", to: "2026-03-17" },
      ctx,
    );
    expect(result.isError).toBe(false);
    const data = jsonData(result);
    expect(data.days).toBe(0);
    expect(data.human).toBe("0 days");
  });

  it("returns error for missing from field", async () => {
    const result = await tool.call(
      { function: "date_diff", to: "2026-03-31" },
      ctx,
    );
    expect(result.isError).toBe(true);
  });

  it("returns error for missing to field", async () => {
    const result = await tool.call(
      { function: "date_diff", from: "2026-03-17" },
      ctx,
    );
    expect(result.isError).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Unknown function
// ---------------------------------------------------------------------------

describe("unknown function", () => {
  it("returns error for unknown function name", async () => {
    const result = await tool.call({ function: "nonexistent" }, ctx);
    expect(result.isError).toBe(true);
    expect(result.asText()).toContain("Unknown function");
  });
});

// ---------------------------------------------------------------------------
// Arg validation
// ---------------------------------------------------------------------------

describe("arg validation", () => {
  it("throws when function field is missing", async () => {
    await expect(tool.call({}, ctx)).rejects.toThrow();
  });
});
