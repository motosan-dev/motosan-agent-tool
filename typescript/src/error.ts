/**
 * Discriminator for the kind of tool error.
 */
export type ToolErrorKind = "missing_field" | "validation" | "parse" | "other";

/**
 * Structured error type for tool operations.
 */
export class ToolError extends Error {
  readonly kind: ToolErrorKind;

  constructor(kind: ToolErrorKind, message: string) {
    super(message);
    this.name = "ToolError";
    this.kind = kind;
  }

  static missingField(field: string): ToolError {
    return new ToolError("missing_field", `Missing required field: ${field}`);
  }

  static validation(message: string): ToolError {
    return new ToolError("validation", message);
  }

  static parse(message: string): ToolError {
    return new ToolError("parse", message);
  }

  static other(message: string): ToolError {
    return new ToolError("other", message);
  }
}
