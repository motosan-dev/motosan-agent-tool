import { ToolError } from "./error.js";

// ---------------------------------------------------------------------------
// ToolContent
// ---------------------------------------------------------------------------

export interface TextContent {
  type: "text";
  data: string;
}

export interface JsonContent {
  type: "json";
  data: unknown;
}

export type ToolContent = TextContent | JsonContent;

// ---------------------------------------------------------------------------
// ToolDef
// ---------------------------------------------------------------------------

export class ToolDef {
  readonly name: string;
  readonly description: string;
  readonly inputSchema: Record<string, unknown>;

  constructor(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
  ) {
    this.name = name;
    this.description = description;
    this.inputSchema = inputSchema;
  }

  /**
   * Validates that the input schema itself is structurally sound.
   * Throws a ToolError if the schema is invalid.
   */
  validateInputSchema(): void {
    if (!this.name || this.name.trim().length === 0) {
      throw ToolError.missingField("name");
    }
    if (!this.description || this.description.trim().length === 0) {
      throw ToolError.missingField("description");
    }
    if (this.inputSchema === null || this.inputSchema === undefined) {
      throw ToolError.missingField("inputSchema");
    }
    if (typeof this.inputSchema !== "object" || Array.isArray(this.inputSchema)) {
      throw ToolError.validation("inputSchema must be a plain object");
    }
  }

  /**
   * Validates that the given args conform to the input schema.
   * Performs basic structural validation (type check for "object" schemas
   * and required-field checking).
   */
  validateArgs(args: unknown): void {
    const schemaType = this.inputSchema["type"];

    if (schemaType === "object") {
      if (args === null || args === undefined || typeof args !== "object" || Array.isArray(args)) {
        throw ToolError.validation("Expected args to be an object");
      }

      const required = this.inputSchema["required"];
      if (Array.isArray(required)) {
        const argsRecord = args as Record<string, unknown>;
        for (const field of required) {
          if (typeof field === "string" && !(field in argsRecord)) {
            throw ToolError.missingField(field);
          }
        }
      }
    }
  }
}

// ---------------------------------------------------------------------------
// ToolResult
// ---------------------------------------------------------------------------

export class ToolResult {
  readonly content: ToolContent[];
  readonly isError: boolean;
  readonly citation?: string;
  readonly injectToContext: boolean;
  readonly durationMs?: number;

  constructor(
    content: ToolContent[],
    isError: boolean,
    citation?: string,
    injectToContext: boolean = false,
    durationMs?: number,
  ) {
    this.content = content;
    this.isError = isError;
    this.citation = citation;
    this.injectToContext = injectToContext;
    this.durationMs = durationMs;
  }

  // --- Static factories ---

  static text(text: string): ToolResult {
    return new ToolResult([{ type: "text", data: text }], false);
  }

  static json(data: unknown): ToolResult {
    return new ToolResult([{ type: "json", data }], false);
  }

  static error(message: string): ToolResult {
    return new ToolResult([{ type: "text", data: message }], true);
  }

  // --- Builder methods (immutable) ---

  withCitation(citation: string): ToolResult {
    return new ToolResult(this.content, this.isError, citation, this.injectToContext, this.durationMs);
  }

  withInject(inject: boolean): ToolResult {
    return new ToolResult(this.content, this.isError, this.citation, inject, this.durationMs);
  }

  withDuration(ms: number): ToolResult {
    return new ToolResult(this.content, this.isError, this.citation, this.injectToContext, ms);
  }

  // --- Helpers ---

  /**
   * Returns the first text content value, or undefined if none exists.
   */
  asText(): string | undefined {
    for (const c of this.content) {
      if (c.type === "text") {
        return c.data;
      }
    }
    return undefined;
  }
}

// ---------------------------------------------------------------------------
// ToolContext
// ---------------------------------------------------------------------------

export class ToolContext {
  readonly callerId: string;
  readonly platform: string;
  readonly extra: Record<string, unknown>;

  constructor(callerId: string, platform: string, extra: Record<string, unknown> = {}) {
    this.callerId = callerId;
    this.platform = platform;
    this.extra = extra;
  }

  static create(callerId: string, platform: string): ToolContext {
    return new ToolContext(callerId, platform);
  }

  /**
   * Returns a new ToolContext with the given key-value pair added to extra.
   */
  with(key: string, value: unknown): ToolContext {
    return new ToolContext(this.callerId, this.platform, { ...this.extra, [key]: value });
  }

  getStr(key: string): string | undefined {
    const v = this.extra[key];
    return typeof v === "string" ? v : undefined;
  }

  getNum(key: string): number | undefined {
    const v = this.extra[key];
    return typeof v === "number" ? v : undefined;
  }

  getBool(key: string): boolean | undefined {
    const v = this.extra[key];
    return typeof v === "boolean" ? v : undefined;
  }
}

// ---------------------------------------------------------------------------
// Tool interface
// ---------------------------------------------------------------------------

export interface Tool {
  def(): ToolDef;
  call(args: unknown, ctx: ToolContext): Promise<ToolResult>;
}
