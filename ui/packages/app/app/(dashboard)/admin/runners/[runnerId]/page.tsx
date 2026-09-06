import type { ReactNode } from "react";
import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { auth } from "@clerk/nextjs/server";
import { Alert } from "@agentsfleet/design-system";
import { ApiError } from "@/lib/api/errors";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { getRunner, listRunnerEvents, listRunnerLeases, type RunnerDetail } from "@/lib/api/runners";
import { RUNNER_LIFECYCLE_EVENT_TYPES } from "@/lib/api/runners-types";
import { resolveRunnerView, runnerPath, RUNNER_VIEW, type RunnerView } from "@/lib/runner-routes";
import {
  CURSOR_PAGE_SIZE_PARAM,
  CURSOR_TRAIL_PARAM,
  PAGE_SIZE_PARAM,
  cursorForTrail,
  cursorTrailFrom,
  pageSizeFrom,
} from "@/lib/pagination/cursor-trail";
import { RunnerHeader } from "./components/RunnerHeader";
import { RunnerSubnavigation } from "./components/RunnerSubnavigation";
import RunnerMetricsStrip from "./components/RunnerMetricsStrip";
import { RunnerSandboxPanel } from "./components/RunnerSandboxPanel";
import { LeaseTable } from "./components/LeaseTable";
import { ActivityTable } from "./components/ActivityTable";
import { RunnerViewedTracker } from "./components/RunnerViewedTracker";
import {
  ACTIVITY_UNAVAILABLE,
  FLEET_FILTER_PARAM,
  LEASES_LINK_STALE,
  LEASES_UNAVAILABLE,
  RESET_LEASE_VIEW_LABEL,
  WORKSPACE_FILTER_PARAM,
} from "./components/runner-copy";

export const dynamic = "force-dynamic";

// Distinguishes "the server refused this address" from "the read failed", which
// `null` alone cannot carry.
const REFUSED = Symbol("leases-refused");

const NOT_ADMIN = "/settings?notice=runners-platform-admin-only";

// Grafana renders only against a configured base — no dead link, no
// placeholder. The runner filter rides a dashboard variable on the href.
const GRAFANA_BASE_ENV = "AGENTSFLEET_GRAFANA_BASE_URL";
const GRAFANA_RUNNER_VARIABLE = "var-runner_id";

function grafanaHrefFor(runnerId: string): string | null {
  const base = process.env[GRAFANA_BASE_ENV];
  if (!base) return null;
  const separator = base.includes("?") ? "&" : "?";
  return `${base}${separator}${GRAFANA_RUNNER_VARIABLE}=${encodeURIComponent(runnerId)}`;
}

export default async function RunnerDetailPage({
  params,
  searchParams,
}: {
  params: Promise<{ runnerId: string }>;
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
  if (!(await hasScope(SCOPE.RUNNER_READ))) redirect(NOT_ADMIN);
  // Read admits the page; write decides which controls exist on it. Resolved
  // here, server-side, rather than in the client header — the browser must not
  // be the one deciding what an operator is allowed to press.
  const canWrite = await hasScope(SCOPE.RUNNER_WRITE);

  const { runnerId } = await params;
  const query: Record<string, string | string[] | undefined> = searchParams ? await searchParams : {};
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const view = resolveRunnerView(typeof query.view === "string" ? query.view : undefined);
  const pageSize = pageSizeFrom(query[PAGE_SIZE_PARAM]);
  const cursor = cursorForTrail(
    cursorTrailFrom(query[CURSOR_TRAIL_PARAM], pageSize, query[CURSOR_PAGE_SIZE_PARAM]),
  );
  const leaseFilters = {
    workspace: singleFilterFrom(query[WORKSPACE_FILTER_PARAM]),
    fleet: singleFilterFrom(query[FLEET_FILTER_PARAM]),
  };

  // The view read needs only the URL id, so it starts beside the runner read
  // instead of behind it — the same shape the fleet console's view-data uses.
  // Its failure mapping is attached at the start, so a runner read that ends in
  // a redirect or not-found leaves no unhandled rejection behind.
  const viewRead = startRunnerViewRead(view, runnerId, token, cursor, pageSize, leaseFilters);
  const runner = await loadRunner(runnerId, token);
  if (!runner) notFound();

  const content = await renderRunnerView(runner, viewRead);

  return (
    <div className="flex min-h-full flex-1 flex-col">
      <RunnerViewedTracker
        runnerId={runner.id}
        liveness={runner.liveness}
        adminState={runner.admin_state}
      />
      <div className="flex min-w-0 flex-col gap-3xl lg:flex-row">
        <div
          aria-hidden="true"
          data-testid="runner-header-alignment-spacer"
          className="hidden lg:block lg:w-56 lg:shrink-0"
        />
        <div className="min-w-0 flex-1">
          <RunnerHeader runner={runner} grafanaHref={grafanaHrefFor(runner.id)} canWrite={canWrite} />
        </div>
      </div>

      <div className="flex min-w-0 flex-1 flex-col gap-3xl lg:flex-row lg:items-stretch">
        <RunnerSubnavigation runnerId={runner.id} activeView={view} />
        <div className="flex min-w-0 flex-1 flex-col">{content}</div>
      </div>
    </div>
  );
}

// The lease filters ride the URL the way the cursor trail does. Anything but a
// single non-empty value — absent, empty, repeated — fails closed to unfiltered,
// mirroring `pageSizeFrom`.
function singleFilterFrom(value: string | string[] | undefined): string | null {
  if (typeof value !== "string") return null;
  return value.length > 0 ? value : null;
}

async function loadRunner(runnerId: string, token: string): Promise<RunnerDetail | null> {
  try {
    return await getRunner(token, runnerId);
  } catch (error: unknown) {
    if (error instanceof ApiError && error.status === 404) return null;
    if (error instanceof ApiError && error.status === 403) redirect(NOT_ADMIN);
    if (error instanceof ApiError && error.status === 401) redirect("/sign-in");
    throw error;
  }
}

type LeaseFilters = { workspace: string | null; fleet: string | null };
type ActivityInitial = Awaited<ReturnType<typeof listRunnerEvents>> | null;
type LeasesInitial = Awaited<ReturnType<typeof listRunnerLeases>> | typeof REFUSED | null;
type RunnerViewRead =
  | { view: typeof RUNNER_VIEW.activity; pageSize: number; initial: Promise<ActivityInitial> }
  | { view: typeof RUNNER_VIEW.leases; pageSize: number; initial: Promise<LeasesInitial> };

// The view switch whose default arm is the page's main object: there is no
// Overview — the runner lands on Leases (the strip riding above the table),
// and Activity is the second rail item, lifecycle records only.
//
// A failed read resolves to null, never to an empty page: the tables' empty
// states mean "this host has no history", and showing that for a database or
// network failure tells the operator the opposite of the truth.
function startRunnerViewRead(
  view: RunnerView,
  runnerId: string,
  token: string,
  cursor: string | null,
  pageSize: number,
  leaseFilters: LeaseFilters,
): RunnerViewRead {
  if (view === RUNNER_VIEW.activity) {
    return {
      view,
      pageSize,
      initial: listRunnerEvents(token, runnerId, {
        limit: pageSize,
        event_type: RUNNER_LIFECYCLE_EVENT_TYPES.join(","),
        ...(cursor ? { starting_after: cursor } : {}),
      }).catch(() => null),
    };
  }
  return {
    view: RUNNER_VIEW.leases,
    pageSize,
    initial: listRunnerLeases(token, runnerId, {
      limit: pageSize,
      ...(cursor ? { starting_after: cursor } : {}),
      ...(leaseFilters.workspace ? { workspace_id: leaseFilters.workspace } : {}),
      ...(leaseFilters.fleet ? { fleet: leaseFilters.fleet } : {}),
    }).catch((error: unknown) => (isRefusedRequest(error) ? REFUSED : null)),
  };
}

// The strip still renders on the Leases view — it reads the runner, which
// succeeded — whatever the lease read did.
async function renderRunnerView(runner: RunnerDetail, read: RunnerViewRead): Promise<ReactNode> {
  if (read.view === RUNNER_VIEW.activity) {
    const initial = await read.initial;
    if (initial === null) return <Alert variant="warning">{ACTIVITY_UNAVAILABLE}</Alert>;
    return <ActivityTable initial={initial} pageSize={read.pageSize} />;
  }
  const initial = await read.initial;
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-3xl">
      <RunnerSandboxPanel runner={runner} />
      <RunnerMetricsStrip runner={runner} />
      {initial === REFUSED ? (
        <Alert variant="warning">
          {LEASES_LINK_STALE}{" "}
          <Link className="underline" href={runnerPath(runner.id, RUNNER_VIEW.leases)}>
            {RESET_LEASE_VIEW_LABEL}
          </Link>
        </Alert>
      ) : initial === null ? (
        <Alert variant="warning">{LEASES_UNAVAILABLE}</Alert>
      ) : (
        <LeaseTable initial={initial} pageSize={read.pageSize} />
      )}
    </div>
  );
}

// The server refused the address, rather than failing to answer it: the
// workspace filter is malformed, or the cursor names a lease outside the
// filtered stream or already pruned by retention. Retrying cannot fix any of
// them, so this case gets a way out instead of an invitation to refresh.
function isRefusedRequest(error: unknown): boolean {
  return error instanceof ApiError && error.status === 400;
}
