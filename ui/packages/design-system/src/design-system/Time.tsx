"use client";

import { type ComponentProps, type ReactNode } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "./Tooltip";
import {
  TIME_DEFAULT_LOCALE,
  coerceDate,
  formatTimeAbsolute,
  toIso,
  visibleTimeLabel,
  type TimeFormat,
} from "./time-utils";

/*
 * Time — wraps <time datetime>. The datetime attribute is canonical
 * ISO-8601 and matches byte-for-byte across SSR + CSR. Visible label
 * comes from one of three formats:
 *   - absolute  → Intl.DateTimeFormat(locale).format(d)        (default)
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
  label: labelOverride,
  tooltipContent,
  className,
  ref,
  ...rest
}: TimeProps) {
  const d = coerceDate(value);

  // Soft-fail on Invalid Date — preserves the pre-migration behaviour of
  // `new Date(bad).toLocaleString()` returning a string instead of throwing.
  // Without this guard `toIso(NaN-Date)` raises RangeError and crashes the
  // surrounding React subtree (server components → 500).
  if (Number.isNaN(d.getTime())) {
    return (
      <time ref={ref} className={className} {...rest}>
        {labelOverride ?? "—"}
      </time>
    );
  }

  const iso = toIso(d);
  const label = labelOverride ?? visibleTimeLabel(value, format, locale, iso);
  const showTooltip = tooltip ?? format === "relative";
  const isRelative = format === "relative";

  const timeEl = (
    <time
      ref={ref}
      dateTime={iso}
      className={className}
      suppressHydrationWarning={isRelative}
      {...rest}
    >
      {label}
    </time>
  );

  if (!showTooltip) return timeEl;

  const tooltipBody = tooltipContent ?? formatTimeAbsolute(value, locale);
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
