/**
 * Unit proof for the acceptance suite's backstop fleet sweep.
 *
 * The sweep only ever runs at the end of a real end-to-end job, so its own
 * failure modes are exactly the ones nobody watches. These tests drive it
 * against stubbed fixtures so the four claims that matter are checked on every
 * `make test-unit-app` instead of being inferred from a green suite:
 * it reaps regardless of fleet name, it reports what it could not delete,
 * one dead fixture user does not shield another's leaks, and it refuses a
 * target that is not disposable.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const DEV_API_URL = "https://api-dev.agentsfleet.net";
const PROD_API_URL = "https://api.agentsfleet.net";

type StubFleet = { id: string; name: string; status: string };

/** Fleets under names no prefix list ever carried — Dimension 1.1's point. */
const UNLISTED_NAMES = ["console-ab12", "pulse-cd34", "nav-ef56"] as const;

/** The collection the library sweep reads and removes through. */
const ENTRIES_PATH = "/library-entries";

/** Entries a leaked run leaves in a fixture workspace's own library. */
const LEAKED_ENTRIES = [
  { id: "entry-1", name: "github-pr-reviewer" },
  { id: "entry-2", name: "github-pr-reviewer" },
] as const;

const listWorkspaces = vi.fn();
const listFleets = vi.fn();
const get = vi.fn();
const del = vi.fn();
const patch = vi.fn();

vi.mock("../tests/e2e/acceptance/fixtures/seed", () => ({
  listWorkspaces: (...args: unknown[]) => listWorkspaces(...args),
  listFleets: (...args: unknown[]) => listFleets(...args),
}));

vi.mock("../tests/e2e/acceptance/fixtures/api-client", () => ({
  clientFor: () => ({ get, delete: del, patch }),
}));

async function loadSweep() {
  return import("../tests/e2e/acceptance/fixtures/teardown");
}

function fleets(...names: readonly string[]): StubFleet[] {
  return names.map((name, i) => ({ id: `fleet-${i}`, name, status: "active" }));
}

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  process.env.NEXT_PUBLIC_API_URL = DEV_API_URL;
  del.mockResolvedValue(undefined);
  patch.mockResolvedValue(undefined);
  get.mockResolvedValue({ items: [] });
});

afterEach(() => {
  delete process.env.NEXT_PUBLIC_API_URL;
});

describe("sweepLeakedFixtureFleets", () => {
  it("test_sweep_reaps_a_fleet_under_any_name", async () => {
    // The predecessor matched six hard-coded prefixes; none of these three
    // names starts with any of them, and all three must still be reaped.
    listWorkspaces.mockResolvedValue([{ id: "ws-1", name: "fixture-workspace" }]);
    listFleets.mockResolvedValue(fleets(...UNLISTED_NAMES));

    const { sweepLeakedFixtureFleets } = await loadSweep();
    const counts = await sweepLeakedFixtureFleets();

    // One workspace per fixture user, three fleets each.
    const fixtureUsers = listWorkspaces.mock.calls.length;
    expect(counts.removed).toBe(UNLISTED_NAMES.length * fixtureUsers);
    expect(counts.failed).toBe(0);
    // Assert each individual fleet reached a delete, not just that the total
    // adds up — a count can be right while the wrong rows were removed.
    const deleted = del.mock.calls.map(([path]) => String(path));
    fleets(...UNLISTED_NAMES).forEach((fleet) => {
      const swept = deleted.some((path) => path.endsWith(fleet.id));
      expect(swept, `expected '${fleet.name}' to be swept`).toBe(true);
    });
  });

  it("test_sweep_reports_failed_deletes", async () => {
    // A fleet the sweep matched but could not delete is the row that keeps
    // waking runners. It used to disappear into a swallowed catch.
    listWorkspaces.mockResolvedValue([{ id: "ws-1", name: "fixture-workspace" }]);
    listFleets.mockResolvedValue(fleets("console-ab12", "pulse-cd34", "nav-ef56"));
    del.mockImplementation((path: string) =>
      path.endsWith("fleet-1") ? Promise.reject(new Error("stuck")) : Promise.resolve(),
    );

    const { sweepLeakedFixtureFleets } = await loadSweep();
    const counts = await sweepLeakedFixtureFleets();

    const fixtureUsers = listWorkspaces.mock.calls.length;
    expect(counts.failed).toBe(fixtureUsers);
    expect(counts.removed).toBe(2 * fixtureUsers);
  });

  it("test_sweep_continues_past_a_dead_fixture", async () => {
    // One purged tenant must not shield every other tenant's leaks.
    listWorkspaces
      .mockRejectedValueOnce(new Error("tenant purged"))
      .mockResolvedValue([{ id: "ws-2", name: "fixture-workspace" }]);
    listFleets.mockResolvedValue(fleets("console-ab12"));

    const { sweepLeakedFixtureFleets } = await loadSweep();
    const counts = await sweepLeakedFixtureFleets();

    // The survivors were still swept, and the dead fixture is not silent.
    expect(counts.removed).toBeGreaterThan(0);
    expect(counts.failed).toBeGreaterThan(0);
  });

  it("test_sweep_refuses_an_unsafe_target", async () => {
    // The guard has to fire before any read, not merely before the delete:
    // listing against production with real fixture credentials is already
    // wrong, and a guard that only wrapped the delete would allow it.
    process.env.NEXT_PUBLIC_API_URL = PROD_API_URL;
    const { sweepLeakedFixtureFleets } = await loadSweep();

    await expect(sweepLeakedFixtureFleets()).rejects.toThrow(/refusing to mass-delete/);
    expect(listWorkspaces).not.toHaveBeenCalled();
    expect(listFleets).not.toHaveBeenCalled();
    expect(del).not.toHaveBeenCalled();
  });
});

/** Serves the entries a workspace still holds, honouring deletes between reads.
 *  The sweep drains by re-reading the first page, so a stub that answers the
 *  same page forever models a server that never removed anything. */
function servePages(all: ReadonlyArray<{ id: string; name?: string }>, pageSize: number) {
  const remaining = new Map(all.map((e) => [e.id, e]));
  get.mockImplementation(() =>
    Promise.resolve({ items: [...remaining.values()].slice(0, pageSize) }),
  );
  del.mockImplementation((path: string) => {
    const id = String(path).split("/").pop() ?? "";
    remaining.delete(id);
    return Promise.resolve();
  });
  return remaining;
}

describe("sweepLeakedFixtureLibraries", () => {
  it("test_library_sweep_is_bounded_and_clears_tenant_rows", async () => {
    // Two near-identical entries under one name: exactly the pile-up that
    // pushed the seeded card off the gallery's first page. A name-scoped
    // sweep would have to know that name; this one does not.
    listWorkspaces.mockResolvedValue([{ id: "ws-1", name: "fixture-workspace" }]);
    servePages([...LEAKED_ENTRIES], 100);

    const { sweepLeakedFixtureLibraries } = await loadSweep();
    const counts = await sweepLeakedFixtureLibraries();

    // One shared store behind every fixture user's workspace, so the first
    // sweep drains it and the rest correctly find nothing left to remove.
    expect(counts.removed).toBe(LEAKED_ENTRIES.length);
    expect(counts.failed).toBe(0);

    // Every read and every delete went through the OWNED collection, which
    // carries no platform row — so the platform catalogue is unreachable from
    // here even by mistake. That is the bound, and it is a property of the
    // path, not of a filter the sweep applies afterwards.
    const read = get.mock.calls.map(([path]) => String(path));
    expect(read.every((path) => path.includes(ENTRIES_PATH))).toBe(true);
    const deleted = del.mock.calls.map(([path]) => String(path));
    LEAKED_ENTRIES.forEach((entry) => {
      const swept = deleted.some(
        (path) => path.includes(ENTRIES_PATH) && path.endsWith(entry.id),
      );
      expect(swept, `expected '${entry.id}' to be swept`).toBe(true);
    });
  });

  it("test_library_sweep_reports_a_failed_removal", async () => {
    // A removal that failed is a row still in the gallery, which is the whole
    // defect the sweep exists to prevent. It is counted, never swallowed.
    listWorkspaces.mockResolvedValue([{ id: "ws-1", name: "fixture-workspace" }]);
    const remaining = servePages([...LEAKED_ENTRIES], 100);
    const removeOrFail = del.getMockImplementation()!;
    del.mockImplementation((path: string) =>
      String(path).endsWith("entry-1")
        ? Promise.reject(new Error("stuck"))
        : removeOrFail(path),
    );
    void remaining;

    const { sweepLeakedFixtureLibraries } = await loadSweep();
    const counts = await sweepLeakedFixtureLibraries();

    // entry-1 refuses every time it is offered; entry-2 goes on the first pass
    // and never comes back, which is what the re-read is for.
    expect(counts.removed).toBe(1);
    expect(counts.failed).toBeGreaterThan(0);
  });

  it("test_library_sweep_drains_every_page_not_just_the_first", async () => {
    // The bug this pins: a single read reaped the first page and called it
    // done. Entries accumulate without bound — that is the whole premise of
    // the milestone — so "one page" and "every entry" part company the moment
    // a fixture workspace passes the page size, and the sweep silently
    // under-reaps the pile it exists to clear.
    // Bigger than one page PER FIXTURE USER: with three users a single-read
    // sweep still gets three reads, so a smaller pile drains by accident and
    // the test proves nothing. This one cannot.
    const PAGE = 100;
    const piled = Array.from({ length: PAGE * 5 + 7 }, (_, i) => ({
      id: `entry-${i}`,
      name: "github-pr-reviewer",
    }));
    listWorkspaces.mockResolvedValue([{ id: "ws-1", name: "fixture-workspace" }]);
    const remaining = servePages(piled, PAGE);

    const { sweepLeakedFixtureLibraries } = await loadSweep();
    const counts = await sweepLeakedFixtureLibraries();

    expect(counts.removed).toBe(piled.length);
    expect(counts.failed).toBe(0);
    expect(remaining.size).toBe(0);
  });

  it("test_library_sweep_stops_when_a_whole_page_refuses", async () => {
    // Re-reading the first page is what makes the drain converge, and it is
    // also what would spin forever if every row on it refused. The pass that
    // removes nothing is the last one.
    listWorkspaces.mockResolvedValue([{ id: "ws-1", name: "fixture-workspace" }]);
    servePages([...LEAKED_ENTRIES], 100);
    del.mockImplementation(() => Promise.reject(new Error("stuck")));

    const { sweepLeakedFixtureLibraries } = await loadSweep();
    const counts = await sweepLeakedFixtureLibraries();

    expect(counts.removed).toBe(0);
    const fixtureUsers = listWorkspaces.mock.calls.length;
    // One read per fixture workspace, not fifty: the stall guard stopped it.
    expect(get.mock.calls.length).toBe(fixtureUsers);
    expect(counts.failed).toBe(LEAKED_ENTRIES.length * fixtureUsers);
  });

  it("test_library_sweep_refuses_an_unsafe_target", async () => {
    // Same guard, same reason as the fleet sweep: listing a real workspace's
    // library with fixture credentials is already wrong, so the refusal has
    // to land before the first read.
    process.env.NEXT_PUBLIC_API_URL = PROD_API_URL;
    const { sweepLeakedFixtureLibraries } = await loadSweep();

    await expect(sweepLeakedFixtureLibraries()).rejects.toThrow(/refusing to mass-delete/);
    expect(listWorkspaces).not.toHaveBeenCalled();
    expect(get).not.toHaveBeenCalled();
    expect(del).not.toHaveBeenCalled();
  });
});

describe("cleanWorkspaceFleets", () => {
  it("test_per_spec_cleanup_still_scopes_by_prefix", async () => {
    // The behaviour §1 must NOT break: parallel workers share one fixture
    // workspace, so a per-spec afterEach that stopped scoping would delete a
    // sibling spec's fleet mid-test.
    listFleets.mockResolvedValue(fleets("kill-aaa", "count-bbb", "kill-ccc"));

    const { cleanWorkspaceFleets } = await loadSweep();
    const counts = await cleanWorkspaceFleets("regular", "ws-1", "kill-");

    expect(counts).toEqual({ removed: 2, failed: 0 });
    const deleted = del.mock.calls.map(([path]) => String(path));
    expect(deleted.some((p) => p.endsWith("fleet-1"))).toBe(false); // count-bbb survived
  });
});
