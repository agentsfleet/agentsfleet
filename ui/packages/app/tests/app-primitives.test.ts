// SSR-based unit tests for app-local presentational primitives that stay
// in this package (DataTable, ActivityFeed). Wave 3 of §3.8 promoted
// StatusCard / EmptyState / Pagination / ConfirmDialog into
// @usezombie/design-system; their coverage moved into co-located tests
// under ui/packages/design-system/src/design-system/*.test.tsx.
//
// Pattern matches tests/app-components.test.ts: renderToStaticMarkup
// + HTML-string assertions. jsdom and @testing-library are intentionally
// not configured in this package.

import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { DataTable, type DataTableColumn } from "../components/ui/data-table";
import { ActivityFeed, type ActivityEvent } from "../components/domain/ActivityFeed";

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
    expect(html).toContain("hidden sm:table-cell");
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
    expect(html).toContain("0a7001");
  });

  it("uses sm:flex-row so rows flow vertically on mobile, horizontally on ≥sm", () => {
    const html = renderToStaticMarkup(
      React.createElement(ActivityFeed, { events }),
    );
    expect(html).toContain("flex-col");
    expect(html).toContain("sm:flex-row");
  });
});
