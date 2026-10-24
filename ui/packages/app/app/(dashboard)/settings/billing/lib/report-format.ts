import type { BillingActivity } from "@/lib/api/billing_report";
import { formatCount } from "@/lib/events/run-figures-format";
import { PROVIDER_MODE } from "@/lib/types";
import { BILLING_ACTIVITY_SETUP } from "@/lib/api/billing_report";
import { MS_PER_SECOND } from "@/lib/utils";

export const SETUP_LABEL = "Setup chat";
export const ALL_FLEETS_LABEL = "All fleets";
export const ALL_MODELS_LABEL = "All models";
const NUMBER_LOCALE = "en-US";
const SECONDS = new Intl.NumberFormat(NUMBER_LOCALE, { maximumFractionDigits: 3 });

export function activityLabel(activity: BillingActivity): string {
  if (activity.activity_type === BILLING_ACTIVITY_SETUP) return SETUP_LABEL;
  return activity.fleet_name ?? activity.fleet_id ?? "Fleet runtime";
}

export function usageLabel(input: number, output: number, runtime: number): string {
  return `${formatCount(input)} input · ${formatCount(output)} output · ${SECONDS.format(runtime / MS_PER_SECOND)}s runtime`;
}

export function providerLabel(postures: string[]): string {
  return postures.includes(PROVIDER_MODE.self_managed)
    ? "Own provider key: model usage is billed by your provider. Credits here cover recorded platform charges."
    : "Model usage and runtime charged to credits.";
}
