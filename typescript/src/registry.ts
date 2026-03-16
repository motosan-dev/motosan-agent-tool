import { type Tool, ToolDef } from "./tool.js";

/**
 * In-memory registry of tools, keyed by their definition name.
 * Methods are async for API compatibility with the Rust crate
 * (which may use async trait methods backed by IO).
 */
export class ToolRegistry {
  private readonly tools: Map<string, Tool> = new Map();

  async register(tool: Tool): Promise<void> {
    const name = tool.def().name;
    this.tools.set(name, tool);
  }

  async get(name: string): Promise<Tool | undefined> {
    return this.tools.get(name);
  }

  async listDefs(): Promise<ToolDef[]> {
    return Array.from(this.tools.values()).map((t) => t.def());
  }

  async size(): Promise<number> {
    return this.tools.size;
  }

  async isEmpty(): Promise<boolean> {
    return this.tools.size === 0;
  }

  async deregister(name: string): Promise<Tool | undefined> {
    const tool = this.tools.get(name);
    this.tools.delete(name);
    return tool;
  }

  async clear(): Promise<void> {
    this.tools.clear();
  }
}
