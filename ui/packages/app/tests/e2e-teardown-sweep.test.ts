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

describe("sweepLeakedFixtureLibraries", () => {
  it("test_library_sweep_is_bounded_and_clears_tenant_rows", async () => {
    // Two near-identical entries under one name: exactly the pile-up that
    // pushed the seeded card off the gallery's first page. A name-scoped
    // sweep would have to know that name; this one does not.
    listWorkspaces.mockResolvedValue([{ id: "ws-1", name: "fixture-workspace" }]);
    get.mockResolvedValue({ items: [...LEAKED_ENTRIES] });

    const { sweepLeakedFixtureLibraries } = await loadSweep();
    const counts = await sweepLeakedFixtureLibraries();

    const fixtureUsers = listWorkspaces.mock.calls.length;
    expect(counts.removed).toBe(LEAKED_ENTRIES.length * fixtureUsers);
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
    get.mockResolvedValue({ items: [...LEAKED_ENTRIES] });
    del.mockImplementation((path: string) =>
      path.endsWith("entry-1") ? Promise.reject(new Error("stuck")) : Promise.resolve(),
    );

    const { sweepLeakedFixtureLibraries } = await loadSweep();
    const counts = await sweepLeakedFixtureLibraries();

    const fixtureUsers = listWorkspaces.mock.calls.length;
    expect(counts.failed).toBe(fixtureUsers);
    expect(counts.removed).toBe(fixtureUsers);
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
