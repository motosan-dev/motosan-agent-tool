import { describe, it, expect } from "vitest";
import { ToolError } from "../src/error.js";

describe("ToolError", () => {
  it("creates a missing_field error", () => {
    const err = ToolError.missingField("name");
    expect(err).toBeInstanceOf(ToolError);
    expect(err).toBeInstanceOf(Error);
    expect(err.kind).toBe("missing_field");
    expect(err.message).toBe("Missing required field: name");
    expect(err.name).toBe("ToolError");
  });

  it("creates a validation error", () => {
    const err = ToolError.validation("bad input");
    expect(err.kind).toBe("validation");
    expect(err.message).toBe("bad input");
  });

  it("creates a parse error", () => {
    const err = ToolError.parse("unexpected token");
    expect(err.kind).toBe("parse");
    expect(err.message).toBe("unexpected token");
  });

  it("creates an other error", () => {
    const err = ToolError.other("something went wrong");
    expect(err.kind).toBe("other");
    expect(err.message).toBe("something went wrong");
  });

  it("can be caught as an Error", () => {
    try {
      throw ToolError.validation("test");
    } catch (e) {
      expect(e).toBeInstanceOf(Error);
      expect(e).toBeInstanceOf(ToolError);
    }
  });
});
