import type { ReactNode } from "react";
import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { auth } from "@clerk/nextjs/server";
import { Alert } from "@agentsfleet/design-system";
import { ApiError } from "@/lib/api/errors";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import {
  getRunner,
  listRunnerEvents,
  listRunnerLeases,
  RUNNER_LIFECYCLE_EVENT_TYPES,
  type RunnerDetail,
} from "@/lib/api/runners";
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
import { LeaseTable } from "./components/LeaseTable";
import { ActivityTable } from "./components/ActivityTable";
import { RunnerViewedTracker } from "./components/RunnerViewedTracker";
import {
  ACTIVITY_UNAVAILABLE,
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
  const workspaceFilter = workspaceFilterFrom(query[WORKSPACE_FILTER_PARAM]);

  const runner = await loadRunner(runnerId, token);
  if (!runner) notFound();

  const content = await loadRunnerView(view, runner, token, cursor, pageSize, workspaceFilter);

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
          <RunnerHeader runner={runner} grafanaHref={grafanaHrefFor(runner.id)} />
        </div>
      </div>

      <div className="flex min-w-0 flex-1 flex-col gap-3xl lg:flex-row lg:items-stretch">
        <RunnerSubnavigation runnerId={runner.id} activeView={view} />
        <div className="flex min-w-0 flex-1 flex-col">{content}</div>
      </div>
    </div>
  );
}

// The workspace filter rides the URL the way the cursor trail does. Anything
// but a single non-empty value — absent, empty, repeated — fails closed to
// unfiltered, mirroring `pageSizeFrom`.
function workspaceFilterFrom(value: string | string[] | undefined): string | null {
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

// The view switch whose default arm is the page's main object: there is no
// Overview — the runner lands on Leases (the strip riding above the table),
// and Activity is the second rail item, lifecycle records only.
async function loadRunnerView(
  view: RunnerView,
  runner: RunnerDetail,
  token: string,
  cursor: string | null,
  pageSize: number,
  workspaceFilter: string | null,
): Promise<ReactNode> {
  // A failed read resolves to null, never to an empty page: the tables' empty
  // states mean "this host has no history", and showing that for a database or
  // network failure tells the operator the opposite of the truth. The strip
  // still renders on the Leases view — it reads the runner, which succeeded.
  if (view === RUNNER_VIEW.activity) {
    const initial = await listRunnerEvents(token, runner.id, {
      limit: pageSize,
      event_type: RUNNER_LIFECYCLE_EVENT_TYPES.join(","),
      ...(cursor ? { starting_after: cursor } : {}),
    }).catch(() => null);
    if (initial === null) return <Alert variant="warning">{ACTIVITY_UNAVAILABLE}</Alert>;
    return <ActivityTable initial={initial} pageSize={pageSize} />;
  }
  const initial = await listRunnerLeases(token, runner.id, {
    limit: pageSize,
    ...(cursor ? { starting_after: cursor } : {}),
    ...(workspaceFilter ? { workspace_id: workspaceFilter } : {}),
  }).catch((error: unknown) => (isRefusedRequest(error) ? REFUSED : null));
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-3xl">
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
        <LeaseTable initial={initial} pageSize={pageSize} />
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
