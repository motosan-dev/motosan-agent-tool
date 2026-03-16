import { describe, it, expect } from "vitest";
import { ToolDef, ToolResult, ToolContext, type Tool } from "../src/tool.js";
import { ToolError } from "../src/error.js";

// ---------------------------------------------------------------------------
// ToolDef
// ---------------------------------------------------------------------------

describe("ToolDef", () => {
  const validSchema: Record<string, unknown> = {
    type: "object",
    properties: { query: { type: "string" } },
    required: ["query"],
  };

  it("stores name, description, and inputSchema", () => {
    const def = new ToolDef("search", "Search the web", validSchema);
    expect(def.name).toBe("search");
    expect(def.description).toBe("Search the web");
    expect(def.inputSchema).toBe(validSchema);
  });

  describe("validateInputSchema", () => {
    it("passes for a valid definition", () => {
      const def = new ToolDef("search", "Search the web", validSchema);
      expect(() => def.validateInputSchema()).not.toThrow();
    });

    it("throws for empty name", () => {
      const def = new ToolDef("", "desc", validSchema);
      expect(() => def.validateInputSchema()).toThrow(ToolError);
    });

    it("throws for whitespace-only name", () => {
      const def = new ToolDef("  ", "desc", validSchema);
      expect(() => def.validateInputSchema()).toThrow(ToolError);
    });

    it("throws for empty description", () => {
      const def = new ToolDef("name", "", validSchema);
      expect(() => def.validateInputSchema()).toThrow(ToolError);
    });

    it("throws if inputSchema is an array", () => {
      const def = new ToolDef("name", "desc", [] as unknown as Record<string, unknown>);
      expect(() => def.validateInputSchema()).toThrow(ToolError);
    });
  });

  describe("validateArgs", () => {
    it("passes when all required fields are present", () => {
      const def = new ToolDef("search", "Search", validSchema);
      expect(() => def.validateArgs({ query: "hello" })).not.toThrow();
    });

    it("throws when a required field is missing", () => {
      const def = new ToolDef("search", "Search", validSchema);
      expect(() => def.validateArgs({})).toThrow(ToolError);
    });

    it("throws when args is not an object", () => {
      const def = new ToolDef("search", "Search", validSchema);
      expect(() => def.validateArgs("string")).toThrow(ToolError);
      expect(() => def.validateArgs(null)).toThrow(ToolError);
      expect(() => def.validateArgs(42)).toThrow(ToolError);
    });

    it("passes for non-object schema types without validation", () => {
      const def = new ToolDef("echo", "Echo", { type: "string" });
      expect(() => def.validateArgs("anything")).not.toThrow();
    });
  });
});

// ---------------------------------------------------------------------------
// ToolResult
// ---------------------------------------------------------------------------

describe("ToolResult", () => {
  it("creates a text result", () => {
    const r = ToolResult.text("hello");
    expect(r.isError).toBe(false);
    expect(r.content).toEqual([{ type: "text", data: "hello" }]);
    expect(r.injectToContext).toBe(false);
    expect(r.citation).toBeUndefined();
    expect(r.durationMs).toBeUndefined();
  });

  it("creates a json result", () => {
    const obj = { key: "value" };
    const r = ToolResult.json(obj);
    expect(r.isError).toBe(false);
    expect(r.content).toEqual([{ type: "json", data: obj }]);
  });

  it("creates an error result", () => {
    const r = ToolResult.error("boom");
    expect(r.isError).toBe(true);
    expect(r.asText()).toBe("boom");
  });

  it("withCitation returns a new instance", () => {
    const r1 = ToolResult.text("hi");
    const r2 = r1.withCitation("src.md");
    expect(r2.citation).toBe("src.md");
    expect(r1.citation).toBeUndefined();
  });

  it("withInject returns a new instance", () => {
    const r1 = ToolResult.text("hi");
    const r2 = r1.withInject(true);
    expect(r2.injectToContext).toBe(true);
    expect(r1.injectToContext).toBe(false);
  });

  it("withDuration returns a new instance", () => {
    const r1 = ToolResult.text("hi");
    const r2 = r1.withDuration(123);
    expect(r2.durationMs).toBe(123);
    expect(r1.durationMs).toBeUndefined();
  });

  it("builder methods can be chained", () => {
    const r = ToolResult.text("hi")
      .withCitation("ref")
      .withInject(true)
      .withDuration(50);
    expect(r.citation).toBe("ref");
    expect(r.injectToContext).toBe(true);
    expect(r.durationMs).toBe(50);
    expect(r.isError).toBe(false);
  });

  it("asText returns undefined when no text content", () => {
    const r = ToolResult.json({ a: 1 });
    expect(r.asText()).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// ToolContext
// ---------------------------------------------------------------------------

describe("ToolContext", () => {
  it("creates via static factory", () => {
    const ctx = ToolContext.create("agent-1", "cli");
    expect(ctx.callerId).toBe("agent-1");
    expect(ctx.platform).toBe("cli");
    expect(ctx.extra).toEqual({});
  });

  it("with() returns a new context with the extra key", () => {
    const ctx1 = ToolContext.create("a", "b");
    const ctx2 = ctx1.with("key", "value");
    expect(ctx2.extra).toEqual({ key: "value" });
    expect(ctx1.extra).toEqual({});
  });

  it("with() can be chained", () => {
    const ctx = ToolContext.create("a", "b")
      .with("str", "hello")
      .with("num", 42)
      .with("bool", true);
    expect(ctx.getStr("str")).toBe("hello");
    expect(ctx.getNum("num")).toBe(42);
    expect(ctx.getBool("bool")).toBe(true);
  });

  it("get helpers return undefined for wrong types", () => {
    const ctx = ToolContext.create("a", "b").with("num", 42);
    expect(ctx.getStr("num")).toBeUndefined();
    expect(ctx.getBool("num")).toBeUndefined();
  });

  it("get helpers return undefined for missing keys", () => {
    const ctx = ToolContext.create("a", "b");
    expect(ctx.getStr("nope")).toBeUndefined();
    expect(ctx.getNum("nope")).toBeUndefined();
    expect(ctx.getBool("nope")).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Tool interface integration
// ---------------------------------------------------------------------------

describe("Tool interface", () => {
  it("can implement and use the Tool interface", async () => {
    const echoTool: Tool = {
      def() {
        return new ToolDef("echo", "Echoes input", {
          type: "object",
          properties: { message: { type: "string" } },
          required: ["message"],
        });
      },
      async call(args: unknown, _ctx: ToolContext): Promise<ToolResult> {
        const { message } = args as { message: string };
        return ToolResult.text(message);
      },
    };

    expect(echoTool.def().name).toBe("echo");

    const ctx = ToolContext.create("test", "unit");
    const result = await echoTool.call({ message: "hi" }, ctx);
    expect(result.asText()).toBe("hi");
    expect(result.isError).toBe(false);
  });
});
