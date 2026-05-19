// CurrentAnalyticsContext — per-Effect-fiber context carried through
// the command pipeline. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/telemetry/analytics-context.ts.
//
// `withAnalyticsContext` merges into the current context (shallow,
// except groups which deep-merges). The Analytics layer reads from
// this reference on every capture so commands don't have to thread
// command_run_id / command / flags_used / flag_values / distinct_id /
// groups by hand.

import { Context, Effect } from "effect";

export interface AnalyticsContext {
  readonly command_run_id?: string;
  readonly command?: string;
  readonly flags_used?: ReadonlyArray<string>;
  readonly flag_values?: Record<string, unknown>;
  readonly distinct_id?: string;
  readonly groups?: {
    readonly organization?: string;
    readonly workspace?: string;
  };
}

export const CurrentAnalyticsContext = Context.Reference<AnalyticsContext>(
  "zombiectl/telemetry/CurrentAnalyticsContext",
  {
    defaultValue: () => ({}),
  },
);

export const withAnalyticsContext = (values: AnalyticsContext) =>
  Effect.updateService(CurrentAnalyticsContext, (current) => {
    const mergedGroups =
      values.groups === undefined
        ? current.groups
        : { ...current.groups, ...values.groups };
    const next: AnalyticsContext = { ...current, ...values };
    if (mergedGroups !== undefined) {
      return { ...next, groups: mergedGroups };
    }
    const { groups: _omit, ...rest } = next;
    return rest;
  });
