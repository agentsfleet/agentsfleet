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
const LIBRARY_CARD = "app/(dashboard)/w/[workspaceId]/fleets/new/LibraryCard.tsx";
const IDENTITY_LINE = "app/(dashboard)/admin/runners/[runnerId]/components/RunnerIdentityLine.tsx";
const APPROVALS_LIST = "app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList.tsx";
const RUNNER_PAGE = "app/(dashboard)/admin/runners/[runnerId]/page.tsx";
const FLEET_TILE = "app/(dashboard)/w/[workspaceId]/fleets/components/FleetTile.tsx";

// The canvas gutter at each breakpoint, in the rem the stylesheet states.
const GUTTER_STEPS = ["1rem", "1.5rem", "2rem", "3rem"];
// The header's own edge: the mobile row's 16px, then from `md` the sidebar
// column's `px-3` plus the wordmark's `ml-2.5`, which is where the brand sits.
const SHELL_EDGE_STEPS = ["1rem", "1.375rem"];
const FLEET_PAGE = "app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx";
const FLEET_SUBNAV = "app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetSubnavigation.tsx";
const FLEET_LOADING = "app/(dashboard)/w/[workspaceId]/fleets/[id]/loading.tsx";
const RUNNER_SUBNAV = "app/(dashboard)/admin/runners/[runnerId]/components/RunnerSubnavigation.tsx";
const RUNNER_LOADING = "app/(dashboard)/admin/runners/[runnerId]/loading.tsx";
// The vertical-rail variant and the spacer that lined the header up with it.
const RAIL_UTILITY = /\blg:(?:flex-col|w-56|w-48|border-r|min-h-full)\b/;
// Anything that would re-introduce a per-element horizontal inset on the two
// elements that must share the gutter.
const HORIZONTAL_PADDING_UTILITY = /\b(?:px|pr|pl)-(?:\d|\[)/;
// A spacing utility carrying a raw number or an arbitrary value rather than one
// of the scale's names. `size-2` and `min-h-5` are sizes, not spacing, and stay;
// so does a `-0` reset, which is on every scale and is how "none" is spelled.
const RAW_SPACING_UTILITY =
  /\b(?:gap|gap-x|gap-y|space-x|space-y|p|px|py|pt|pb|pl|pr|m|mx|my|mt|mb|ml|mr)-(?:\[|[1-9]\d*(?:\.\d+)?|0\.\d+)/g;

describe("the canvas gutter is stated once", () => {
  it("declares the gutter and the shell edge as tokens the shell can share", () => {
    const css = read(GLOBALS);
    for (const step of GUTTER_STEPS) {
      expect(css).toContain(`--app-canvas-gutter: ${step}`);
    }
    for (const step of SHELL_EDGE_STEPS) {
      expect(css).toContain(`--app-shell-edge: ${step}`);
    }
  });

  it("reserves the scrollbar so the content edge does not move when a page scrolls", () => {
    // Measured before: a scrolling page put its content 48px from the left and
    // 54px from the right, because the 6px scrollbar came out of the content
    // box only once there was something to scroll.
    const canvas = read(GLOBALS).split(".app-dashboard-canvas {")[1]?.split("}")[0] ?? "";
    // both-edges, not bare `stable`: reserving on the end edge alone is what
    // made a scrolling page read 48 left against 54 right. And the gutter is
    // never subtracted back out of the padding — whether the browser reserves
    // it at all varies, and subtracting where it does not costs 6px of the
    // token on every route.
    expect(canvas).toMatch(/padding-inline:\s*var\(--app-canvas-gutter\)/);
    expect(canvas).not.toMatch(/padding-inline:\s*calc/);
    expect(canvas).toMatch(/scrollbar-gutter:\s*stable both-edges/);
  });

  it("gives the header's trailing cluster the edge its leading cluster has", () => {
    // Measured before: the brand dot sat 22px in from the left while the
    // avatar sat 48px in from the right at 1920px, because the trailing
    // cluster was pinned to the canvas's content edge (gutter plus whatever
    // scrollbar the platform reserved) instead of mirroring the sidebar's
    // own inset. The header keeps one edge of its own, stated once, and no
    // longer needs a probe to measure the canvas's scrollbar for it.
    const trailing = read(GLOBALS).split(".app-shell-trailing {")[1]?.split("}")[0] ?? "";
    expect(trailing).toMatch(/padding-right:\s*var\(--app-shell-edge\)/);
    expect(trailing).not.toContain("--app-canvas-gutter");
    expect(trailing).not.toContain("--app-scrollbar");
    expect(read(SHELL_FRAME)).not.toContain("CanvasScrollbarProbe");
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

describe("a page's section nav is a strip, not a second sidebar", () => {
  it.each([
    ["the fleet sections", FLEET_SUBNAV],
    ["the runner sections", RUNNER_SUBNAV],
    ["the fleet loading silhouette", FLEET_LOADING],
    ["the runner loading silhouette", RUNNER_LOADING],
  ])("renders %s as one strip at every width", (_name, path) => {
    // Measured before: from `lg` the nav became a 224px column beside the main
    // sidebar, with a 224px spacer above it so the title started to its right.
    // That read as two sidebars and a blank block top-left, and cost the
    // transcript 224px. The strip the page already rendered below `lg` is the
    // shape at every width now.
    expect(read(path)).not.toMatch(RAIL_UTILITY);
  });

  it.each([
    ["the fleet page", FLEET_PAGE, "fleet-header-alignment-spacer"],
    ["the runner page", RUNNER_PAGE, "runner-header-alignment-spacer"],
  ])("lets %s's header span the full content width", (_name, path, spacer) => {
    const source = read(path);
    expect(source).not.toContain(spacer);
    expect(source).not.toMatch(RAIL_UTILITY);
    expect(source).not.toContain("lg:flex-row");
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

describe("the cards on the wall and in the gallery use the named scale", () => {
  it.each([
    ["the fleet tile", FLEET_TILE],
    ["the library card", LIBRARY_CARD],
  ])("gives %s no raw spacing number", (_name, path) => {
    // Measured before: the library card ran 15 · 12 · 16 · 19 · 15 down its
    // length and the tile carried six values, most of them raw Tailwind
    // numbers that no token names.
    const source = read(path);
    const inJsx = source
      .split("\n")
      .filter((line) => line.includes("className=") || line.includes('"'))
      .join("\n");
    expect(inJsx.match(RAW_SPACING_UTILITY) ?? []).toEqual([]);
  });

  it("leaves the library card's inset to the Card that frames it", () => {
    const card = read(LIBRARY_CARD).split("<Card")[1]?.split(">")[0] ?? "";
    expect(card).not.toMatch(/\bp-/);
  });

  it("states a credential requirement as a fact, not a warning", () => {
    // Amber is this system's warning colour. A fleet naming the credential it
    // will ask for is a fact about the fleet, not a fault in the workspace.
    const source = read(LIBRARY_CARD);
    expect(source).toContain('const REQUIRES_PREFIX = "requires:"');
    expect(source).not.toMatch(/variant="amber"/);
  });
});

describe("a block does not carry the gap that belongs to its column", () => {
  it("gives the runner page the same column gap as the blocks inside it", () => {
    // Measured before: the page root stacked its header row and view row with
    // no gap at all, so the only separation was the identity line's own margin.
    const source = read(RUNNER_PAGE);
    const rootGap = source.split("flex min-h-full flex-1 flex-col")[1]?.split('"')[0] ?? "";
    expect(rootGap).toContain("gap-3xl");
    expect(source).toContain("flex min-w-0 flex-1 flex-col gap-3xl");
  });

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
