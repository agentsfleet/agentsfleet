import { describe, expect, it } from "vitest";
import { hasLibraryWriteScope } from "../app/(dashboard)/w/[workspaceId]/fleets/scope";

describe("test_add_template_hidden_without_scope", () => {
  it("accepts a space-delimited library:write scope string", () => {
    expect(hasLibraryWriteScope({ scopes: "fleet:read library:write" })).toBe(true);
  });

  it("accepts a scopes array", () => {
    expect(hasLibraryWriteScope({ scopes: ["fleet:read", "library:write"] })).toBe(true);
  });

  it("rejects missing, malformed, or unrelated scopes", () => {
    expect(hasLibraryWriteScope(null)).toBe(false);
    expect(hasLibraryWriteScope({ scopes: "fleet:read" })).toBe(false);
    expect(hasLibraryWriteScope({ scopes: ["fleet:read"] })).toBe(false);
    expect(hasLibraryWriteScope({ scopes: 42 })).toBe(false);
  });
});
