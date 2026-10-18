import { expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { goToPage, routerPush, leaseView, lease, MOCK_PATHNAME, LEASES_PAST_ONE_PAGE,
  OLDER_INSTANT_MS, NEWER_INSTANT_MS } from "@/tests/helpers/runner-leases";
import { APPLY_LEASE_FILTER_LABEL, FLEET_LABEL, WORKSPACE_LABEL } from "./runner-copy";
import { LeaseTable } from "./LeaseTable";
import { agentDisplayName } from "@/lib/fleets/agent-label";

// TooltipProvider stands in for the ancestor supplied by the app layout.
it("LeaseTable > test_lease_table_orders_live_leases_first", () => {
  render(
    <LeaseTable
      initial={{
        items: [
          lease({ id: "settled-1", outcome: "succeeded" }),
          lease({ id: "running-1", outcome: "running", wall_ms: null }),
          lease({ id: "settled-2", outcome: "failed", failure_label: "oom_kill" }),
          lease({ id: "running-2", outcome: "running", wall_ms: null }),
        ],
        total: 4,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  const rows = screen.getAllByRole("row").slice(1); // drop the header row
  const runningIndexes = rows
    .map((row, index) => (row.textContent?.includes("Running") ? index : -1))
    .filter((index) => index >= 0);
  const settledIndexes = rows
    .map((row, index) => (row.textContent?.includes("Running") ? -1 : index))
    .filter((index) => index >= 0);
  expect(runningIndexes).toHaveLength(2);
  expect(Math.max(...runningIndexes)).toBeLessThan(Math.min(...settledIndexes));
});

it("LeaseTable > test_lease_table_failed_row_renders_failure_sentence", () => {
  render(
    <LeaseTable
      initial={{
        items: [
          lease({
            id: "failed-1",
            outcome: "failed",
            failure_label: "oom_kill",
            failure_detail: "Container exceeded its 2 GiB memory limit and was terminated.",
          }),
        ],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  // The shared plain-English sentence renders; the machine tag never does.
  expect(screen.getByText("Ran out of memory")).toBeTruthy();
  expect(screen.getByText("Container exceeded its 2 GiB memory limit and was terminated.")).toBeTruthy();
  expect(screen.queryByText(/oom_kill/)).toBeNull();
});

it("LeaseTable > test_lease_table_expired_row_states_reclaim", () => {
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "expired-1", outcome: "expired" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  expect(screen.getByText("Lease not renewed")).toBeTruthy();
  expect(
    screen.getByText("This runner stopped renewing; the work was re-leased to another runner."),
  ).toBeTruthy();
});

it("LeaseTable > suppresses the outcome fabrication for a lease with no recorded event", () => {
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "unknown-1", outcome: "unknown" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  expect(screen.getByText("Status not recorded")).toBeTruthy();
});

it("LeaseTable > test_review_lease_renders_lease_facts (from the row, and released on close)", () => {
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "row-open-1", outcome: "succeeded" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  // The fencing token lives only in Review lease — absent until a row is
  // activated, present after, gone again once the panel closes.
  expect(screen.queryByText("1,884")).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: `View lease for ${agentDisplayName("fleet-1")}`}));
  fireEvent.click(screen.getByRole("button", { name: "Technical details" }));
  expect(screen.getByText("1,884")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /close/i }));
  expect(screen.queryByText("1,884")).toBeNull();
});

it("LeaseTable > should hand the pager to the cursor feed when more pages exist", () => {
  leaseView.hasNext = true;
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "page-1-lease" })],
        total: LEASES_PAST_ONE_PAGE,
        next_cursor: "page-2-cursor",
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  expect(goToPage).toHaveBeenCalledWith(2);
});

it("LeaseTable > a lease row names its agent and keeps the uuid in title", () => {
  // Dimension 5.4. The cell reads the callsign — the name every other agent
  // column uses — even though the row carries the fleet's given name, and
  // the UUID stays reachable on hover rather than painted in the cell.
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "named-1", fleet_name: "Search Services", fleet_id: "fleet-1" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  const cell = screen.getByText(agentDisplayName("fleet-1"));
  expect(cell.textContent).toMatch(/^AGENT [A-Z]+-[0-9A-F]{4}$/);
  expect(cell.getAttribute("title")).toBe("fleet-1");
  expect(screen.queryByText("Search Services")).toBeNull();
});

it("LeaseTable > should name a fleet deleted out from under its leases, keeping the id on hover", () => {
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "orphanish-1", fleet_name: null, fleet_id: "fleet-gone-1" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  // The defensive render the cascade failure-mode names: never a blank cell
  // and never a fabricated name. The callsign is derived from the id, so it
  // is the name the fleet carried while it existed rather than one invented
  // here, and the id itself is still reachable.
  const cell = screen.getByText(agentDisplayName("fleet-gone-1"));
  expect(cell).toBeTruthy();
  expect(cell.getAttribute("title")).toBe("fleet-gone-1");
});

it("LeaseTable > renders the workspace cell as a shortened link carrying the full id", () => {
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "ws-cell-1", workspace_id: "ws-0123456789" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  expect(screen.getByRole("columnheader", {name: WORKSPACE_LABEL})).toBeTruthy();
  const link = screen.getByRole("link", { name: "ws-01234…" });
  expect(link.getAttribute("href")).toBe("/w/ws-0123456789/fleets");
  expect(link.getAttribute("title")).toBe("ws-0123456789");
  // No filter in the URL — no chip to clear.
  expect(screen.queryByRole("button", { name: "Clear workspace filter" })).toBeNull();
});

it("LeaseTable > opens the workspace, not Review lease, when the workspace link is clicked", () => {
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "ws-link-1", workspace_id: "ws-0123456789" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  fireEvent.click(screen.getByRole("link", { name: "ws-01234…" }));
  // The cell sits inside a row whose own click means "review this lease".
  // Leaving for the workspace is a different intent, so the link stops the
  // click travelling — otherwise the operator lands on the fleet wall with a
  // panel they never asked for left open behind them.
  expect(screen.queryByRole("dialog", { name: "Review lease" })).toBeNull();
});

it("LeaseTable > applies both filter fields and drops the cursor trail with the old result set", () => {
  leaseView.search = "c=page-2-cursor&cps=25";
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "ws-filter-1", workspace_id: "ws-0123456789" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  fireEvent.change(screen.getByRole("textbox", {name: WORKSPACE_LABEL}), {target: {value: "ws-0123456789"}});
  fireEvent.change(screen.getByRole("textbox", {name: FLEET_LABEL}), {target: {value: "pr-reviewer"}});
  fireEvent.click(screen.getByRole("button", { name: APPLY_LEASE_FILTER_LABEL }));
  expect(routerPush).toHaveBeenCalledTimes(1);
  const url = routerPush.mock.calls[0]?.[0] as string;
  const params = new URLSearchParams(url.split("?")[1]);
  expect(params.get("workspace")).toBe("ws-0123456789");
  expect(params.get("fleet")).toBe("pr-reviewer");
  // The cursors walked the OLD result set; they cannot page the new one.
  expect(params.getAll("c")).toHaveLength(0);
  expect(params.get("cps")).toBeNull();
  // Filtering narrows the feed; it must not also open Review lease.
  expect(screen.queryByRole("dialog", { name: "Review lease" })).toBeNull();
});

it("LeaseTable > clears one filter without disturbing the other", () => {
  leaseView.search = "workspace=ws-0123456789&fleet=pr-reviewer";
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "ws-both-1", workspace_id: "ws-0123456789" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  // Two chips, two independent clears — dropping the workspace must leave the
  // fleet narrowing in place, or the operator loses work they did not undo.
  fireEvent.click(screen.getByRole("button", { name: "Clear workspace filter" }));
  const url = routerPush.mock.calls[0]?.[0] as string;
  const params = new URLSearchParams(url.split("?")[1]);
  expect(params.get("workspace")).toBeNull();
  expect(params.get("fleet")).toBe("pr-reviewer");
});

it("LeaseTable > clears the fleet without disturbing the workspace — the mirror of the case above", () => {
  leaseView.search = "workspace=ws-0123456789&fleet=pr-reviewer";
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "fleet-drop-1", workspace_id: "ws-0123456789" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  // The other half of the independent-clear claim. Only asserting the
  // workspace direction would leave a `clearFleet` that dropped both filters
  // — or the wrong one — entirely unobserved.
  fireEvent.click(screen.getByRole("button", { name: "Clear fleet filter" }));
  const url = routerPush.mock.calls[0]?.[0] as string;
  const params = new URLSearchParams(url.split("?")[1]);
  expect(params.get("fleet")).toBeNull();
  expect(params.get("workspace")).toBe("ws-0123456789");
});

it("LeaseTable > drops both filters at once through clear-all, not one at a time", () => {
  leaseView.search = "workspace=ws-0123456789&fleet=pr-reviewer";
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "clear-all-1", workspace_id: "ws-0123456789" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  // Back to the bare path: no query at all, so neither filter nor a stale
  // cursor trail survives the reset.
  fireEvent.click(screen.getByRole("button", { name: "Clear all filters" }));
  expect(routerPush).toHaveBeenCalledWith(MOCK_PATHNAME, { scroll: true });
});

it("LeaseTable > shows the active filter as a chip and clears back to the unfiltered feed", () => {
  leaseView.search = "workspace=ws-0123456789";
  render(
    <LeaseTable
      initial={{
        items: [lease({ id: "ws-chip-1", workspace_id: "ws-0123456789" })],
        total: 1,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  expect(screen.getByText("Workspace ws-01234…")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Clear workspace filter" }));
  expect(routerPush).toHaveBeenCalledWith(MOCK_PATHNAME, { scroll: true });
});

it("LeaseTable > should sort by Created through the standard header control", () => {
  render(
    <LeaseTable
      initial={{
        items: [
          lease({ id: "older", fleet_id: "fleet-older", created_at: new Date(OLDER_INSTANT_MS).toISOString(), outcome: "succeeded" }),
          lease({ id: "newer", fleet_id: "fleet-newer", created_at: new Date(NEWER_INSTANT_MS).toISOString(), outcome: "succeeded" }),
        ],
        // total unknown (null) — the pager renders without a fabricated count.
        total: null,
        next_cursor: null,
      }}
      pageSize={25}
    />, { wrapper: TooltipProvider });
  const orderOf = () =>
    screen
      .getAllByRole("row")
      .slice(1)
      .map((row) => (row.textContent?.includes(agentDisplayName("fleet-older")) ? "older" : "newer"));
  fireEvent.click(screen.getByRole("button", { name: /created/i }));
  const firstSort = orderOf();
  fireEvent.click(screen.getByRole("button", { name: /created/i }));
  // Two clicks walk both directions of the same comparator: the order must
  // invert, proving the column sorts on the lease's real timestamp.
  expect(orderOf()).toEqual([...firstSort].reverse());
});
