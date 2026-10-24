import type { BillingActivity, BillingReportOptions } from "@/lib/api/billing_report";
import { BILLING_ACTIVITY_SETUP, BILLING_PERIOD_MONTH } from "@/lib/api/billing_report";
import {
  CURSOR_PAGE_SIZE_PARAM, CURSOR_TRAIL_PARAM, PAGE_SIZE_PARAM,
  cursorForTrail, cursorTrailFrom, pageSizeFrom,
} from "@/lib/pagination/cursor-trail";

export const BILLING_QUERY = {
  period: "period", fleet: "fleet", model: "model", through: "through_at",
  activity: "activity", detailCursor: "dc",
} as const;
export const BILLING_ALL = "all";
export const BILLING_SETUP = BILLING_ACTIVITY_SETUP;
export const DETAIL_PAGE_SIZE = 25;
export type BillingSearch = Record<string, string | string[] | undefined>;

/** Work type is part of activity identity; separate identifier spaces can overlap. */
export function activityKey(activity: Pick<BillingActivity, "activity_type" | "id">): string {
  return `${activity.activity_type}:${activity.id}`;
}

/** A malformed explicit filter reaches the daemon's validation instead of widening the read. */
export function reportOptions(query: BillingSearch): BillingReportOptions {
  const limit = pageSizeFrom(query[PAGE_SIZE_PARAM]);
  const cursor = cursorForTrail(cursorTrailFrom(query[CURSOR_TRAIL_PARAM], limit, query[CURSOR_PAGE_SIZE_PARAM]));
  const fleet = first(query[BILLING_QUERY.fleet]);
  return {
    period: first(query[BILLING_QUERY.period]) ?? BILLING_PERIOD_MONTH,
    fleet_id: fleet && fleet !== BILLING_ALL && fleet !== BILLING_SETUP ? fleet : undefined,
    activity_type: fleet === BILLING_SETUP ? BILLING_ACTIVITY_SETUP : undefined,
    model: first(query[BILLING_QUERY.model]),
    through_at: first(query[BILLING_QUERY.through]),
    limit, starting_after: cursor ?? undefined,
  };
}

/** Filters start a fresh report; old continuations and expanded details cannot bleed across. */
export function changedFilter(params: URLSearchParams, key: string, value: string): URLSearchParams {
  const next = new URLSearchParams(params);
  for (const name of [CURSOR_TRAIL_PARAM, CURSOR_PAGE_SIZE_PARAM, BILLING_QUERY.through, BILLING_QUERY.activity, BILLING_QUERY.detailCursor]) next.delete(name);
  if (value === BILLING_ALL) next.delete(key);
  else next.set(key, value);
  return next;
}

export function selectedActivity(params: URLSearchParams, id: string, through: string): URLSearchParams {
  const next = new URLSearchParams(params);
  next.delete(BILLING_QUERY.detailCursor);
  next.set(BILLING_QUERY.through, through);
  if (id) next.set(BILLING_QUERY.activity, id);
  else next.delete(BILLING_QUERY.activity);
  return next;
}

export function detailTrail(value: string | string[] | undefined): string[] {
  return cursorTrailFrom(value, DETAIL_PAGE_SIZE, String(DETAIL_PAGE_SIZE));
}

export function nextDetailPage(params: URLSearchParams, target: number, cursor: string | null): URLSearchParams | null {
  const current = params.getAll(BILLING_QUERY.detailCursor);
  const trail = target === current.length + 2 && cursor ? [...current, cursor]
    : target === current.length && current.length > 0 ? current.slice(0, -1) : null;
  if (trail === null) return null;
  const next = new URLSearchParams(params);
  next.delete(BILLING_QUERY.detailCursor);
  for (const entry of trail) next.append(BILLING_QUERY.detailCursor, entry);
  return next;
}

export function first(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? value[0] : value;
}
