import { describe, expect, it } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  it("joins plain string inputs with single spaces", () => {
    expect(cn("a", "b", "c")).toBe("a b c");
  });

  it("drops falsy inputs (false, null, undefined, empty string, 0)", () => {
    expect(cn("a", false, null, undefined, "", 0, "b")).toBe("a b");
  });

  it("keeps a conditionally-included class and drops the falsy branch", () => {
    const active = true;
    const disabled = false;
    expect(cn("base", active && "is-active", disabled && "is-disabled")).toBe(
      "base is-active",
    );
  });

  it("flattens a nested array of class values", () => {
    expect(cn("a", ["b", "c"], "d")).toBe("a b c d");
  });

  it("flattens deeply nested arrays and skips their falsy members", () => {
    expect(cn(["a", ["b", false, ["c", null]]], "d")).toBe("a b c d");
  });

  it("returns an empty string when every input is falsy", () => {
    expect(cn(false, null, undefined, "", 0)).toBe("");
  });

  // Every size token in the scale, not just one. `text-reading` was added to
  // theme.css and NOT to the font-size group, so `cn()` silently dropped it and
  // the chat transcript rendered at the inherited 15px while looking correct in
  // source. A per-token assertion is what turns that into a failing test rather
  // than a measurement someone has to take in a browser.
  it("test_ds_cn_keeps_every_font_size_token_beside_a_colour", () => {
    const scale = [
      "text-display-xl", "text-display-lg", "text-display-md", "text-heading",
      "text-eyebrow", "text-reading", "text-body-lg", "text-body", "text-body-sm",
      "text-label", "text-mono", "text-conversation", "text-chat-title",
    ];
    for (const token of scale) {
      expect(cn(token, "text-foreground")).toBe(`${token} text-foreground`);
    }
  });

  it("test_ds_cn_merges_and_keeps_fontsize — resolves a Tailwind conflict last-wins", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
    expect(cn("text-body", "text-heading")).toBe("text-heading");
  });

  it("lets caller spacing override semantic defaults without leaving competing classes", () => {
    expect(cn("px-md", "px-lg")).toBe("px-lg");
    expect(cn("px-md", "px-3")).toBe("px-3");
    expect(cn("gap-md", "gap-2.5")).toBe("gap-2.5");
    expect(cn("px-3", "px-md")).toBe("px-md");
    expect(cn("md:px-md", "md:px-lg")).toBe("md:px-lg");
  });

  it("test_ds_cn_merges_and_keeps_fontsize — a semantic font-size token survives beside a color token", () => {
    // Without the extended font-size class group, tailwind-merge classifies
    // text-eyebrow as a text-color and drops it in favor of the real color.
    expect(cn("text-eyebrow", "text-muted-foreground")).toBe(
      "text-eyebrow text-muted-foreground",
    );
  });
});
