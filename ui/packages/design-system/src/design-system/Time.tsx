"use client";

import { useSyncExternalStore, type ComponentProps, type ReactNode } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "./Tooltip";
import {
  TIME_DEFAULT_LOCALE,
  coerceDate,
  formatTimeAbsolute,
  toIso,
  visibleTimeLabel,
  formatTimeRange,
  TIME_INVALID_FALLBACK,
  type TimeFormat,
} from "./time-utils";

/*
 * Time — wraps <time datetime>. The datetime attribute is canonical
 * ISO-8601 and matches byte-for-byte across SSR + CSR. Visible label
 * uses the shared formatter:
 *   - absolute, date, compact, minute, clock → local calendar and clock
 *   - month     → the report's calendar-month identity
 *   - relative  → "X minutes ago" / "in X hours" (deterministic given now)
 *   - datetime  → the full ISO string
 *
 * `relative` opts into `suppressHydrationWarning` because the visible
 * label depends on Date.now() which differs between SSR and the first
 * client paint. Tooltip defaults on for `relative` so the absolute form
 * is always one hover away.
 *
 * Pure formatting helpers (formatTimeAbsolute / formatTimeRelative) live
 * in ./time-utils so server-side callers can import them without pulling
 * the client-component entry point onto the server bundle.
 */

export interface TimeProps
  extends Omit<ComponentProps<"time">, "dateTime" | "children"> {
  value: string | Date;
  format?: TimeFormat;
  tooltip?: boolean;
  locale?: string;
  /** Optional named timezone; local browser time is the default. */
  timeZone?: string;
  /** Override the visible label while keeping the canonical datetime attr. */
  label?: string;
  /** Override the tooltip body. Defaults to the absolute formatted string. */
  tooltipContent?: ReactNode;
}

export function Time({
  value,
  format = "absolute",
  tooltip,
  locale = TIME_DEFAULT_LOCALE,
  timeZone,
  label: labelOverride,
  tooltipContent,
  className,
  ref,
  ...rest
}: TimeProps) {
  const localReady = useClientTimeReady();
  const d = coerceDate(value);

  // Invalid instants use the shared fallback before toIso can throw and
  // take down the surrounding page.
  if (Number.isNaN(d.getTime())) {
    return (
      <time ref={ref} className={className} {...rest}>
        {labelOverride ?? "—"}
      </time>
    );
  }

  const iso = toIso(d);
  const local = !timeZone && ["absolute", "clock", "date", "compact", "minute"].includes(format);
  const label = labelOverride ?? (local && !localReady ? TIME_INVALID_FALLBACK : visibleTimeLabel(value, format, locale, iso, timeZone));
  const showTooltip = tooltip ?? format === "relative";
  const isRelative = format === "relative";
  // Local labels wait for the browser snapshot. Relative labels can move
  // during hydration as the elapsed clock advances.
  const labelIsEnvironmentDependent = isRelative || format === "clock";

  const timeEl = (
    <time
      ref={ref}
      dateTime={iso}
      className={className}
      suppressHydrationWarning={labelIsEnvironmentDependent}
      {...rest}
    >
      {label}
    </time>
  );

  if (!showTooltip) return timeEl;

  const tooltipBody = tooltipContent ?? formatTimeAbsolute(value, locale, timeZone);
  return (
    <Tooltip>
      <TooltipTrigger asChild>{timeEl}</TooltipTrigger>
      <TooltipContent>{tooltipBody}</TooltipContent>
    </Tooltip>
  );
}

export {
  formatTimeAbsolute,
  formatTimeRelative,
  type TimeFormat,
} from "./time-utils";

export default Time;

const subscribeLocalTime = () => () => undefined;
const clientTimeReady = () => true;
const serverTimeReady = () => false;
export function useClientTimeReady() {
  return useSyncExternalStore(subscribeLocalTime, clientTimeReady, serverTimeReady);
}

/** Resolve on the browser after hydration so server time cannot label the report. */
export function TimeRange({from, through, locale = TIME_DEFAULT_LOCALE}: {from: string | Date; through: string | Date; locale?: string}) {
  const localReady = useClientTimeReady();
  return <Time value={through} label={localReady ? formatTimeRange(from, through, locale) : TIME_INVALID_FALLBACK} />;
}
