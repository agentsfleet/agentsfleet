import { describe, expect, it } from "vitest";
import { hasPluginWriteScope } from "../app/(dashboard)/w/[workspaceId]/fleets/scope";

describe("test_add_template_hidden_without_scope", () => {
  it("accepts a space-delimited plugins:write scope string", () => {
    expect(hasPluginWriteScope({ scopes: "fleet:read plugins:write" })).toBe(true);
  });

  it("accepts a scopes array", () => {
    expect(hasPluginWriteScope({ scopes: ["fleet:read", "plugins:write"] })).toBe(true);
  });

  it("rejects missing, malformed, or unrelated scopes", () => {
    expect(hasPluginWriteScope(null)).toBe(false);
    expect(hasPluginWriteScope({ scopes: "fleet:read" })).toBe(false);
    expect(hasPluginWriteScope({ scopes: ["fleet:read"] })).toBe(false);
    expect(hasPluginWriteScope({ scopes: 42 })).toBe(false);
  });
});
