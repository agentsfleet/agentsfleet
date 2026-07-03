import { describe, expect, it } from "vitest";
import { hasTemplateWriteScope } from "../app/(dashboard)/fleets/scope";

describe("test_add_template_hidden_without_scope", () => {
  it("accepts a space-delimited template:write scope string", () => {
    expect(hasTemplateWriteScope({ scopes: "fleet:read template:write" })).toBe(true);
  });

  it("accepts a scopes array", () => {
    expect(hasTemplateWriteScope({ scopes: ["fleet:read", "template:write"] })).toBe(true);
  });

  it("rejects missing, malformed, or unrelated scopes", () => {
    expect(hasTemplateWriteScope(null)).toBe(false);
    expect(hasTemplateWriteScope({ scopes: "fleet:read" })).toBe(false);
    expect(hasTemplateWriteScope({ scopes: ["fleet:read"] })).toBe(false);
    expect(hasTemplateWriteScope({ scopes: 42 })).toBe(false);
  });
});
