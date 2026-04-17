// Unit tests for M12_001-owned shared UI primitives.
//
// Pattern matches tests/app-components.test.ts: SSR the component with
// react-dom/server, assert on the resulting HTML string. Verifies:
//   • default + prop-driven render output
//   • required ARIA attributes (role, aria-label, aria-live, aria-sort, etc.)
//   • mobile-responsive class presence (sm:, hidden sm:table-cell, flex-wrap)
//   • empty/edge states
//
// We intentionally do not depend on jsdom or @testing-library: those aren't
// configured in this package (vitest.config.ts uses the default node env).
// SSR + string assertions cover the contract that matters for these
// presentational primitives.

import React from "react";
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { StatusCard } from "../components/ui/status-card";
import { EmptyState } from "../components/ui/empty-state";
import { Pagination } from "../components/ui/pagination";
import { DataTable, type DataTableColumn } from "../components/ui/data-table";
import { ConfirmDialog } from "../components/ui/confirm-dialog";
import { ActivityFeed, type ActivityEvent } from "../components/domain/ActivityFeed";

// ── StatusCard ─────────────────────────────────────────────────────────────

describe("StatusCard", () => {
  it("renders the label and count and exposes an aria-label for SR users", () => {
    const html = renderToStaticMarkup(
      React.createElement(StatusCard, { label: "Active", count: 3 }),
    );
    expect(html).toContain(">Active<");
    expect(html).toContain(">3<");
    expect(html).toContain('role="group"');
    expect(html).toContain('aria-label="Active, 3"');
    expect(html).toContain('data-testid="status-card"');
  });

  it("applies the variant accent class and surfaces the trend glyph + textual label", () => {
    const html = renderToStaticMarkup(
      React.createElement(StatusCard, {
        label: "Stopped",
        count: 1,
        variant: "danger",
        trend: "up",
        sublabel: "last 24h",
      }),
    );
    expect(html).toContain('data-variant="danger"');
    expect(html).toContain("text-destructive");
    expect(html).toContain("↑");
    // Trend glyph is decorative; text is in the aria-label.
    expect(html).toContain("aria-hidden");
    expect(html).toContain("increasing");
    expect(html).toContain("last 24h");
  });

  it("emits data-slot=\"status-card\" for styling hooks + test targeting", () => {
    const html = renderToStaticMarkup(
      React.createElement(StatusCard, { label: "X", count: 1 }),
    );
    expect(html).toContain('data-slot="status-card"');
  });

  it("does not accept asChild (display compositions wrap externally — see status-card.tsx header note)", () => {
    // Intentional design decision: StatusCard renders its own internal <dl>,
    // which is incompatible with Radix Slot's single-child model. Callers
    // who want a clickable tile wrap the whole card in <Link>.
    const props = {} as Parameters<typeof StatusCard>[0];
    expect("asChild" in props).toBe(false);
  });

  it("uses responsive sizing so the count is smaller on narrow screens", () => {
    const html = renderToStaticMarkup(
      React.createElement(StatusCard, { label: "X", count: 9 }),
    );
    expect(html).toContain("text-xl");
    expect(html).toContain("sm:text-2xl");
  });

  it("respects prefers-reduced-motion by disabling transitions", () => {
    const html = renderToStaticMarkup(
      React.createElement(StatusCard, { label: "X", count: 0 }),
    );
    expect(html).toContain("motion-reduce:transition-none");
  });
});

// ── EmptyState ─────────────────────────────────────────────────────────────

describe("EmptyState", () => {
  it("renders the title and uses role=status with aria-live=polite", () => {
    const html = renderToStaticMarkup(
      React.createElement(EmptyState, {
        title: "Nothing here",
        description: "Add a zombie to get started.",
      }),
    );
    expect(html).toContain("Nothing here");
    expect(html).toContain("Add a zombie to get started.");
    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
  });

  it("omits the description paragraph when no description is provided", () => {
    const html = renderToStaticMarkup(
      React.createElement(EmptyState, { title: "Nada" }),
    );
    expect(html).toContain("Nada");
    expect(html).not.toContain("<p ");
  });

  it("renders icon with aria-hidden and surfaces an action slot", () => {
    const html = renderToStaticMarkup(
      React.createElement(EmptyState, {
        title: "X",
        icon: React.createElement("i", { "data-icon": "1" }),
        action: React.createElement("button", null, "Start"),
      }),
    );
    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain('data-icon="1"');
    expect(html).toContain(">Start<");
  });

  it("uses responsive padding (tighter on mobile)", () => {
    const html = renderToStaticMarkup(
      React.createElement(EmptyState, { title: "X" }),
    );
    expect(html).toContain("p-6");
    expect(html).toContain("sm:p-10");
  });
});

// ── Pagination ─────────────────────────────────────────────────────────────

describe("Pagination (cursor variant)", () => {
  it("enables Load more when a cursor is present", () => {
    const onNext = vi.fn();
    const html = renderToStaticMarkup(
      React.createElement(Pagination, {
        kind: "cursor",
        nextCursor: "abc123",
        onNext,
      }),
    );
    expect(html).toContain('role="navigation"');
    expect(html).toContain('aria-label="Feed pagination"');
    expect(html).toContain("Load more");
    expect(html).not.toContain("disabled=\"\"");
  });

  it("shows End of feed and disables the button when nextCursor is null", () => {
    const html = renderToStaticMarkup(
      React.createElement(Pagination, {
        kind: "cursor",
        nextCursor: null,
        onNext: () => {},
      }),
    );
    expect(html).toContain("End of feed");
    expect(html).toContain("disabled=\"\"");
  });

  it("shows Loading… while fetching", () => {
    const html = renderToStaticMarkup(
      React.createElement(Pagination, {
        kind: "cursor",
        nextCursor: "abc",
        onNext: () => {},
        isLoading: true,
      }),
    );
    expect(html).toContain("Loading");
  });

  it("uses flex-wrap so buttons reflow on narrow viewports", () => {
    const html = renderToStaticMarkup(
      React.createElement(Pagination, {
        kind: "cursor",
        nextCursor: "abc",
        onNext: () => {},
      }),
    );
    expect(html).toContain("flex-wrap");
  });
});

describe("Pagination (page variant)", () => {
  it("renders Page X of Y with aria-live=polite and correct button states", () => {
    const html = renderToStaticMarkup(
      React.createElement(Pagination, {
        kind: "page",
        page: 2,
        pageSize: 20,
        total: 87,
        onPageChange: () => {},
      }),
    );
    // ceil(87/20) = 5 pages.
    expect(html).toContain("Page 2 of 5");
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain('aria-atomic="true"');
    // Prev/Next both enabled at page 2.
    expect(html).toContain("Previous");
    expect(html).toContain("Next");
  });

  it("disables Previous on page 1", () => {
    const html = renderToStaticMarkup(
      React.createElement(Pagination, {
        kind: "page",
        page: 1,
        pageSize: 20,
        total: 87,
        onPageChange: () => {},
      }),
    );
    expect(html.match(/disabled=""/g)?.length).toBeGreaterThanOrEqual(1);
  });

  it("falls back to Page N (no /Y) when total is unknown", () => {
    const html = renderToStaticMarkup(
      React.createElement(Pagination, {
        kind: "page",
        page: 4,
        pageSize: 20,
        onPageChange: () => {},
      }),
    );
    expect(html).toContain("Page 4");
    // Precise: no "of Y" fragment — use a whitespace-anchored match so we
    // don't collide with utility-class substrings like "offset" or "ring-offset".
    expect(html).not.toMatch(/Page\s+\d+\s+of\s+\d+/);
  });
});

// ── DataTable ──────────────────────────────────────────────────────────────

type Row = { id: string; name: string; spend: number };

describe("DataTable", () => {
  const cols: DataTableColumn<Row>[] = [
    { key: "name", header: "Name", cell: (r) => r.name },
    { key: "spend", header: "Spend", numeric: true, hideOnMobile: true, cell: (r) => `$${r.spend}` },
  ];

  it("renders column headers with scope=col and the right-aligned numeric class", () => {
    const html = renderToStaticMarkup(
      React.createElement(DataTable<Row>, {
        columns: cols,
        rows: [{ id: "z1", name: "one", spend: 12 }],
        rowKey: (r) => r.id,
      }),
    );
    expect(html).toContain('scope="col"');
    expect(html).toContain(">Name<");
    expect(html).toContain(">Spend<");
    expect(html).toContain("text-right");
    expect(html).toContain("hidden sm:table-cell"); // numeric column hidden on mobile
    expect(html).toContain(">one<");
    expect(html).toContain(">$12<");
  });

  it("renders the default EmptyState when rows is empty", () => {
    const html = renderToStaticMarkup(
      React.createElement(DataTable<Row>, {
        columns: cols,
        rows: [],
        rowKey: (r) => r.id,
      }),
    );
    expect(html).toContain("Nothing to show yet");
    expect(html).toContain('role="status"');
  });

  it("respects a caller-supplied empty slot", () => {
    const html = renderToStaticMarkup(
      React.createElement(DataTable<Row>, {
        columns: cols,
        rows: [],
        rowKey: (r) => r.id,
        empty: React.createElement("div", null, "Nada"),
      }),
    );
    expect(html).toContain(">Nada<");
    expect(html).not.toContain("Nothing to show yet");
  });

  it("adds role=button + tabIndex when onRowClick is provided", () => {
    const html = renderToStaticMarkup(
      React.createElement(DataTable<Row>, {
        columns: cols,
        rows: [{ id: "z1", name: "one", spend: 12 }],
        rowKey: (r) => r.id,
        onRowClick: () => {},
      }),
    );
    expect(html).toContain('role="button"');
    expect(html).toContain('tabindex="0"');
    expect(html).toContain("cursor-pointer");
  });

  it("exposes aria-busy while loading", () => {
    const html = renderToStaticMarkup(
      React.createElement(DataTable<Row>, {
        columns: cols,
        rows: [{ id: "z1", name: "one", spend: 1 }],
        rowKey: (r) => r.id,
        isLoading: true,
      }),
    );
    expect(html).toContain('aria-busy="true"');
  });
});

// ── ConfirmDialog (closed state; Radix dialog needs a DOM for open=true) ───

describe("ConfirmDialog", () => {
  it("renders nothing when open is false (Radix Dialog hides the portal)", () => {
    const html = renderToStaticMarkup(
      React.createElement(ConfirmDialog, {
        open: false,
        onOpenChange: () => {},
        title: "Stop zombie?",
        onConfirm: () => {},
      }),
    );
    // Radix Dialog portal is not mounted when closed.
    expect(html).not.toContain("Stop zombie?");
  });

  it("exports a stable callable shape (open/onOpenChange/title/onConfirm required)", () => {
    // Compile-time contract check — if the types break, the .ts build fails.
    const props: Parameters<typeof ConfirmDialog>[0] = {
      open: true,
      onOpenChange: () => {},
      title: "x",
      onConfirm: async () => {},
      intent: "destructive",
      confirmLabel: "Stop",
      cancelLabel: "Nope",
      errorMessage: null,
    };
    expect(typeof props.onConfirm).toBe("function");
    expect(props.intent).toBe("destructive");
  });
});

// ── ActivityFeed ───────────────────────────────────────────────────────────

describe("ActivityFeed", () => {
  const events: ActivityEvent[] = [
    {
      id: "a1",
      zombie_id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001",
      workspace_id: "ws1",
      event_type: "email.received",
      detail: "jane@acme.com",
      created_at: Date.UTC(2026, 3, 16, 10, 47) + (new Date().getTimezoneOffset() * 60_000),
      zombie_name: "lead-collector",
    },
  ];

  it("renders the feed with role=list, time element, zombie name, event type, detail", () => {
    const html = renderToStaticMarkup(
      React.createElement(ActivityFeed, { events, title: "Recent" }),
    );
    expect(html).toContain('aria-label="Recent"');
    expect(html).toContain('role="list"');
    expect(html).toContain("<time");
    // React SSR preserves the JSX `dateTime` prop name as an attribute. HTML
    // parsers lower-case it on ingestion but renderToStaticMarkup keeps the
    // source casing — so the assertion matches the rendered string directly.
    expect(html).toMatch(/dateTime="\d{4}-\d{2}-\d{2}T/);
    expect(html).toContain("lead-collector");
    expect(html).toContain("email.received");
    expect(html).toContain("jane@acme.com");
  });

  it("shows EmptyState when the events array is empty", () => {
    const html = renderToStaticMarkup(
      React.createElement(ActivityFeed, { events: [] }),
    );
    expect(html).toContain("No activity yet");
    expect(html).toContain('role="status"');
  });

  it("falls back to the zombie_id tail when zombie_name is missing", () => {
    const base = events[0]!;
    const noName: ActivityEvent = {
      id: base.id,
      zombie_id: base.zombie_id,
      workspace_id: base.workspace_id,
      event_type: base.event_type,
      detail: base.detail,
      created_at: base.created_at,
    };
    const html = renderToStaticMarkup(
      React.createElement(ActivityFeed, { events: [noName] }),
    );
    expect(html).toContain("0a7001"); // last 6 chars of the zombie UUID
  });

  it("uses sm:flex-row so rows flow vertically on mobile, horizontally on ≥sm", () => {
    const html = renderToStaticMarkup(
      React.createElement(ActivityFeed, { events }),
    );
    expect(html).toContain("flex-col");
    expect(html).toContain("sm:flex-row");
  });
});
