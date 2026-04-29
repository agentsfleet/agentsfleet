// SSR-based unit tests for app-local presentational primitives that stay
// in this package (DataTable). Wave 3 of §3.8 promoted StatusCard /
// EmptyState / Pagination / ConfirmDialog into @usezombie/design-system;
// their coverage moved into co-located tests under
// ui/packages/design-system/src/design-system/*.test.tsx.
//
// ActivityFeed was retired in slice 10 (M42); the dashboard now renders
// `core.zombie_events` directly via <EventsList />. Co-located rendering
// tests for EventsList live in components/domain/EventsList.test.tsx.
//
// Pattern matches tests/app-components.test.ts: renderToStaticMarkup
// + HTML-string assertions. jsdom and @testing-library are intentionally
// not configured in this package.

import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";

import { DataTable, type DataTableColumn } from "../components/ui/data-table";

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

// ActivityFeed was deleted in slice 10 (M42); EventsList replaced it.
// EventsList is a "use client" component with hooks, so it does not
// SSR via renderToStaticMarkup. Its coverage lives in the dashboard
// integration tests + lib/api/events.test.ts.
