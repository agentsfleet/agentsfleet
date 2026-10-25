import type { BillingActivity } from "@/lib/api/billing_report";
import { formatCount } from "@/lib/events/run-figures-format";
import { PROVIDER_MODE } from "@/lib/types";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing-report-values";
import { MS_PER_SECOND } from "@/lib/utils";

export const SETUP_LABEL = "Setup chat";
export const ALL_FLEETS_LABEL = "All fleets & setup chats";
export const ALL_MODELS_LABEL = "All models";
const NUMBER_LOCALE = "en-US";
const SECONDS = new Intl.NumberFormat(NUMBER_LOCALE, { maximumFractionDigits: 3 });

export function runtimeLabel(runtime: number): string {
  return `${runtimeSecondsLabel(runtime)}s`;
}

export function runtimeSecondsLabel(runtime: number): string {
  return SECONDS.format(runtime / MS_PER_SECOND);
}

export function activityLabel(activity: BillingActivity): string {
  if (activity.activity_type === BILLING_ACTIVITY_SETUP) return SETUP_LABEL;
  return activity.fleet_name ?? activity.fleet_id ?? "Fleet runtime";
}

export function usageLabel(input: number, output: number, runtime: number): string {
  return `${formatCount(input)} input · ${formatCount(output)} output · ${runtimeLabel(runtime)} runtime`;
}

export function providerLabel(postures: string[]): string | null {
  return postures.includes(PROVIDER_MODE.self_managed)
    ? "Own provider key: model usage is billed by your provider. Credits here cover recorded platform charges."
    : null;
}
