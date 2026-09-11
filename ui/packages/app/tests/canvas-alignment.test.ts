/**
 * One horizontal rhythm for the dashboard, and no block inventing its own edge.
 *
 * Every gap in here was measured on the running app before it was fixed, and
 * each test names the number it found. They are class and stylesheet
 * assertions on purpose: the defects were all a second copy of a spacing
 * scale, so what has to stay true is that there is only one copy.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const read = (path: string): string =>
  readFileSync(resolve(import.meta.dirname, "..", path), "utf8");

const GLOBALS = "app/globals.css";
const SHELL_FRAME = "components/layout/ShellFrame.tsx";
const BALANCE_CARD = "app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx";
const TRIGGER_PANEL = "app/(dashboard)/w/[workspaceId]/fleets/[id]/components/TriggerPanel.tsx";
const IDENTITY_LINE = "app/(dashboard)/admin/runners/[runnerId]/components/RunnerIdentityLine.tsx";
const APPROVALS_LIST = "app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList.tsx";

// The canvas gutter at each breakpoint, in the rem the stylesheet states.
const GUTTER_STEPS = ["1rem", "1.5rem", "2rem", "3rem"];
// Anything that would re-introduce a per-element horizontal inset on the two
// elements that must share the gutter.
const HORIZONTAL_PADDING_UTILITY = /\b(?:px|pr|pl)-(?:\d|\[)/;

describe("the canvas gutter is stated once", () => {
  it("declares the gutter and the scrollbar width as tokens the shell can share", () => {
    const css = read(GLOBALS);
    expect(css).toMatch(/--app-scrollbar:\s*6px/);
    for (const step of GUTTER_STEPS) {
      expect(css).toContain(`--app-canvas-gutter: ${step}`);
    }
  });

  it("reserves the scrollbar so the content edge does not move when a page scrolls", () => {
    // Measured before: a scrolling page put its content 48px from the left and
    // 54px from the right, because the 6px scrollbar came out of the content
    // box only once there was something to scroll.
    const canvas = read(GLOBALS).split(".app-dashboard-canvas {")[1]?.split("}")[0] ?? "";
    expect(canvas).toMatch(
      /padding-inline:\s*calc\(var\(--app-canvas-gutter\)\s*-\s*var\(--app-scrollbar\)\)/,
    );
    expect(canvas).toMatch(/scrollbar-gutter:\s*stable both-edges/);
  });

  it("lands the header's trailing cluster on the canvas's content edge", () => {
    // Measured before: the avatar sat 24px outboard of the content below it at
    // 1920px, because the header ran pr-4/md:pr-6 against the canvas's
    // px-4/sm:px-6/md:px-8/2xl:px-12.
    const trailing = read(GLOBALS).split(".app-shell-trailing {")[1]?.split("}")[0] ?? "";
    expect(trailing).toMatch(/padding-right:\s*var\(--app-canvas-gutter\)/);
  });

  it("leaves neither shell element a horizontal inset of its own", () => {
    const shell = read(SHELL_FRAME);
    const canvasLine = shell.split("\n").find((l) => l.includes("app-dashboard-canvas")) ?? "";
    const trailingLine = shell.split("\n").find((l) => l.includes("app-shell-trailing")) ?? "";
    expect(canvasLine).not.toBe("");
    expect(trailingLine).not.toBe("");
    expect(canvasLine).not.toMatch(HORIZONTAL_PADDING_UTILITY);
    expect(trailingLine).not.toMatch(HORIZONTAL_PADDING_UTILITY);
  });
});

describe("a card's inset is the card's, once", () => {
  it.each([
    ["the balance card", BALANCE_CARD],
    ["the trigger panel", TRIGGER_PANEL],
  ])("does not pad %s inside a Card that already padded it", (_name, path) => {
    // Measured before: the balance card's content sat 40px from its border
    // (Card's p-2xl plus CardContent's p-4) where every other card sits at 24.
    const content = read(path).split("<CardContent")[1]?.split(">")[0] ?? "";
    expect(content).not.toMatch(/\bp[xytblr]?-\d/);
  });
});

describe("a block does not carry the gap that belongs to its column", () => {
  it("ends the runner identity line without a margin of its own", () => {
    // Measured before: mb-2xl made the runner header-to-content gap 24px where
    // every other gap on that page is 32.
    // The component's own root is its last `return (` — the first belongs to
    // the capability-sentence helper above it.
    const blocks = read(IDENTITY_LINE).split("return (");
    const root = blocks[blocks.length - 1]?.split(">")[0] ?? "";
    expect(root).toContain("flex flex-col");
    expect(root).not.toMatch(/\bmb-/);
  });

  it("gives the approvals section header no margin the other pages lack", () => {
    // Measured before: this was the only SectionHeader in the app carrying
    // className="mb-md", so its header-to-table gap read 24 against 16.
    const header = read(APPROVALS_LIST).split("<SectionHeader")[1]?.split(">")[0] ?? "";
    expect(header).not.toMatch(/\bmb-/);
  });

  it("finds no section header anywhere that adds a vertical margin", () => {
    // The rule generalised: SectionHeader owns the gap to its body, so no call
    // site anywhere may add a vertical margin to it.
    const offenders: string[] = [];
    const roots = ["app", "components"];
    const walk = (dir: string): string[] => {
      const here = resolve(import.meta.dirname, "..", dir);
      const out: string[] = [];
      for (const entry of readdirSync(here)) {
        const full = resolve(here, entry);
        if (statSync(full).isDirectory()) out.push(...walk(`${dir}/${entry}`));
        else if (entry.endsWith(".tsx") && !entry.includes(".test.")) out.push(`${dir}/${entry}`);
      }
      return out;
    };
    for (const file of roots.flatMap(walk)) {
      const source = read(file);
      for (const chunk of source.split("<SectionHeader").slice(1)) {
        const attrs = chunk.split(">")[0] ?? "";
        if (/\bm[byt]-/.test(attrs)) offenders.push(file);
      }
    }
    expect(offenders).toEqual([]);
  });
});
