import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
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

  it("test_ds_cn_merges_and_keeps_fontsize — resolves a Tailwind conflict last-wins", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
    expect(cn("text-body", "text-heading")).toBe("text-heading");
  });

  it("test_ds_cn_merges_and_keeps_fontsize — a semantic font-size token survives beside a color token", () => {
    // Without the extended font-size class group, tailwind-merge classifies
    // text-eyebrow as a text-color and drops it in favor of the real color.
    expect(cn("text-eyebrow", "text-muted-foreground")).toBe(
      "text-eyebrow text-muted-foreground",
    );
  });
});

describe("cn — single workspace declaration", () => {
  // vitest runs from the package dir (jsdom rewrites import.meta.url, so
  // cwd is the reliable anchor); one level up is ui/packages.
  const PACKAGES_ROOT = resolve(process.cwd(), "..");
  const SKIPPED_DIRS = new Set(["node_modules", ".next", "dist", "build", "coverage"]);
  // Matches a declaration, not a re-export or an import — `export { cn }`
  // and `import { cn }` deliberately do not hit.
  const CN_DECLARATION = /export (?:function|const) cn\b/;

  function tsSourcesUnder(dir: string): string[] {
    const out: string[] = [];
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        if (SKIPPED_DIRS.has(entry.name)) continue;
        out.push(...tsSourcesUnder(`${dir}/${entry.name}`));
        continue;
      }
      if (/\.tsx?$/.test(entry.name)) out.push(`${dir}/${entry.name}`);
    }
    return out;
  }

  it("test_single_cn_export — exactly one cn declaration exists across ui/packages", () => {
    // Fail loud if the cwd anchor resolved somewhere unexpected — a scan of
    // the wrong tree would pass while guarding nothing.
    expect(existsSync(resolve(PACKAGES_ROOT, "design-system"))).toBe(true);
    expect(existsSync(resolve(PACKAGES_ROOT, "app"))).toBe(true);
    const hits = tsSourcesUnder(PACKAGES_ROOT).filter((file) =>
      CN_DECLARATION.test(readFileSync(file, "utf8")),
    );
    expect(hits).toHaveLength(1);
    expect(hits[0]).toContain("design-system/src/utils.ts");
  });
});
