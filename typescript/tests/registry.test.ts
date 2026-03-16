import { describe, it, expect, beforeEach } from "vitest";
import { ToolRegistry } from "../src/registry.js";
import { ToolDef, ToolResult, ToolContext, type Tool } from "../src/tool.js";

function makeTool(name: string): Tool {
  return {
    def() {
      return new ToolDef(name, `${name} tool`, { type: "object", properties: {} });
    },
    async call(_args: unknown, _ctx: ToolContext): Promise<ToolResult> {
      return ToolResult.text(`${name} called`);
    },
  };
}

describe("ToolRegistry", () => {
  let registry: ToolRegistry;

  beforeEach(() => {
    registry = new ToolRegistry();
  });

  it("starts empty", async () => {
    expect(await registry.isEmpty()).toBe(true);
    expect(await registry.size()).toBe(0);
  });

  it("registers and retrieves a tool", async () => {
    const tool = makeTool("search");
    await registry.register(tool);

    const found = await registry.get("search");
    expect(found).toBeDefined();
    expect(found!.def().name).toBe("search");
    expect(await registry.size()).toBe(1);
    expect(await registry.isEmpty()).toBe(false);
  });

  it("overwrites a tool with the same name", async () => {
    await registry.register(makeTool("search"));
    await registry.register(makeTool("search"));
    expect(await registry.size()).toBe(1);
  });

  it("returns undefined for unknown tool", async () => {
    expect(await registry.get("nope")).toBeUndefined();
  });

  it("lists all definitions", async () => {
    await registry.register(makeTool("alpha"));
    await registry.register(makeTool("beta"));

    const defs = await registry.listDefs();
    const names = defs.map((d) => d.name).sort();
    expect(names).toEqual(["alpha", "beta"]);
  });

  it("deregisters a tool and returns it", async () => {
    await registry.register(makeTool("search"));
    const removed = await registry.deregister("search");
    expect(removed).toBeDefined();
    expect(removed!.def().name).toBe("search");
    expect(await registry.size()).toBe(0);
  });

  it("deregister returns undefined for unknown tool", async () => {
    expect(await registry.deregister("nope")).toBeUndefined();
  });

  it("clears all tools", async () => {
    await registry.register(makeTool("a"));
    await registry.register(makeTool("b"));
    await registry.clear();
    expect(await registry.isEmpty()).toBe(true);
  });
});
