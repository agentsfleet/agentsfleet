/**
 * What the server says a fleet was charged, and what the dashboard should name
 * each model: the two facts the Usage journey checks the screen against.
 */
import { clientFor, type ClientHandle } from "./api-client";
import { modelLabelsOf, type ModelLabels } from "@/lib/model-labels";

const REPORT_PATH = "/v1/tenants/me/billing/reports/current";
const CATALOGUE_PATH = "/v1/models";
const CATALOGUE_PAGE_BOUND = 40;

export interface FleetUsage {
  readonly fleetName: string;
  readonly chargedNanos: bigint;
  readonly models: readonly string[];
  readonly activityCount: number;
}

interface ReportRow { key: string | null; label: string | null; models: string[]; totals: { charged_nanos: string; activity_count: number } }
interface Report { spending: { by_fleet: ReportRow[] } }
interface CataloguePage { models: { id: string; display_name: string }[]; next_cursor: string | null }

/** The fleet's settled group in this month's report, or null before settlement. */
export async function fleetUsage(handle: ClientHandle, fleetId: string): Promise<FleetUsage | null> {
  const query = new URLSearchParams({ fleet_id: fleetId });
  const report = await clientFor(handle).get<Report>(`${REPORT_PATH}?${query}`);
  const row = report.spending.by_fleet.find(group => group.key === fleetId);
  if (!row || BigInt(row.totals.charged_nanos) === 0n) return null;
  return { fleetName: row.label ?? fleetId, chargedNanos: BigInt(row.totals.charged_nanos),
    models: row.models, activityCount: row.totals.activity_count };
}

/** The labels the dashboard's shared store resolves, read from the same complete catalogue. */
export async function catalogueLabels(handle: ClientHandle): Promise<ModelLabels> {
  const models: CataloguePage["models"] = [];
  let cursor: string | null = null;
  for (let page = 0; page < CATALOGUE_PAGE_BOUND; page += 1) {
    const query: string = cursor ? `?advanced=true&starting_after=${encodeURIComponent(cursor)}` : "?advanced=true";
    const library: CataloguePage = await clientFor(handle).get<CataloguePage>(`${CATALOGUE_PATH}${query}`);
    models.push(...library.models);
    cursor = library.next_cursor;
    if (cursor === null) return modelLabelsOf(models);
  }
  throw new Error("the model catalogue did not end within its page bound");
}
