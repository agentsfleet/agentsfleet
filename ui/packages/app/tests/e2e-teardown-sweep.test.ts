/**
 * Unit proof for the acceptance suite's backstop fleet sweep.
 *
 * The sweep only ever runs at the end of a real end-to-end job, so its own
 * failure modes are exactly the ones nobody watches. These tests drive it
 * against stubbed fixtures so the four claims that matter are checked on every
 * `make test-coverage-app` instead of being inferred from a green suite:
 * it reaps regardless of fleet name, it reports what it could not delete,
 * one dead fixture user does not shield another's leaks, and it refuses a
 * target that is not disposable.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FLEET, FLEET_ID, DRAFT_ID, CANDIDATE_ID } from "./helpers/draft-fixtures";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { SOAK_FLEET_PREFIX } from "./e2e/acceptance/fixtures/soak-scenario";

const DEV_API_URL = "https://api-dev.agentsfleet.net";
const PROD_API_URL = "https://api.agentsfleet.net";

type StubFleet = { id: string; name: string; status: string };

/** Fleets under names no prefix list ever carried — Dimension 1.1's point. */
const UNLISTED_NAMES = ["console-ab12", "pulse-cd34", "nav-ef56"] as const;

const FLEET_IDS = [FLEET_ID, DRAFT_ID, CANDIDATE_ID] as const;

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
  return names.map((name, i) => ({ id: FLEET_IDS[i] ?? FLEET_ID, name, status: AGENTSFLEET_STATUS.ACTIVE }));
}

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  process.env.NEXT_PUBLIC_API_URL = DEV_API_URL;
  del.mockResolvedValue(undefined);
  get.mockResolvedValue({...FLEET, status: AGENTSFLEET_STATUS.ACTIVE});
});

afterEach(() => {
  delete process.env.NEXT_PUBLIC_API_URL;
});

describe("sweepLeakedFixtureFleets", () => {
  it("preserves soak evidence while removing unrelated leaked fleets", async () => {
    listWorkspaces.mockResolvedValue([{id: "ws-1", name: "fixture-workspace"}]);
    listFleets.mockResolvedValue(fleets(`${SOAK_FLEET_PREFIX}single-preserved`, UNLISTED_NAMES[0]));
    const {sweepLeakedFixtureFleets} = await loadSweep();

    const counts = await sweepLeakedFixtureFleets();

    expect(counts).toEqual({removed: listWorkspaces.mock.calls.length, failed: 0});
    expect(get.mock.calls.some(([path]) => String(path).endsWith(FLEET_ID))).toBe(false);
    expect(del.mock.calls.some(([path]) => String(path).endsWith(FLEET_ID))).toBe(false);
    expect(del.mock.calls.some(([path]) => String(path).endsWith(DRAFT_ID))).toBe(true);
  });

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
    // DELETE ends live work itself: the sweep never archives first.
    expect(patch).not.toHaveBeenCalled();
    expect(del).toHaveBeenCalledWith(expect.stringContaining(FLEET_ID), {"If-Match": FLEET.etag});
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
      path.endsWith(DRAFT_ID) ? Promise.reject(new Error("stuck")) : Promise.resolve(),
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

describe("cleanWorkspaceFleets", () => {
  it("test_per_spec_cleanup_still_scopes_by_prefix", async () => {
    // The behaviour §1 must NOT break: parallel workers share one fixture
    // workspace, so a per-spec afterEach that stopped scoping would delete a
    // sibling spec's fleet mid-test.
    listFleets.mockResolvedValue(fleets("sweep-aaa", "count-bbb", "sweep-ccc"));

    const { cleanWorkspaceFleets } = await loadSweep();
    const counts = await cleanWorkspaceFleets("regular", "ws-1", "sweep-");

    expect(counts).toEqual({ removed: 2, failed: 0 });
    const deleted = del.mock.calls.map(([path]) => String(path));
    expect(deleted.some((p) => p.endsWith(DRAFT_ID))).toBe(false); // count-bbb survived
  });
});
