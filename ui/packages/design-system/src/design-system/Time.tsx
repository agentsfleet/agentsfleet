"use client";

import { type ComponentProps } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "./Tooltip";

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
 */

export type TimeFormat = "absolute" | "relative" | "datetime";

export interface TimeProps
  extends Omit<ComponentProps<"time">, "dateTime" | "children"> {
  value: string | Date;
  format?: TimeFormat;
  tooltip?: boolean;
  locale?: string;
  /** Override the visible label while keeping the canonical datetime attr. */
  label?: string;
}

const DEFAULT_LOCALE = "en-US";

const ABSOLUTE_OPTIONS: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "short",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
};

function coerceDate(value: string | Date): Date {
  return value instanceof Date ? value : new Date(value);
}

function toIso(d: Date): string {
  return d.toISOString();
}

export function formatTimeAbsolute(
  value: string | Date,
  locale: string = DEFAULT_LOCALE,
): string {
  const d = coerceDate(value);
  return new Intl.DateTimeFormat(locale, ABSOLUTE_OPTIONS).format(d);
}

export function formatTimeRelative(
  value: string | Date,
  now: Date = new Date(),
): string {
  const d = coerceDate(value);
  const deltaSec = Math.round((d.getTime() - now.getTime()) / 1000);
  const abs = Math.abs(deltaSec);
  const past = deltaSec <= 0;

  const unit =
    abs < 60 ? { n: abs, label: "second" }
    : abs < 3_600 ? { n: Math.floor(abs / 60), label: "minute" }
    : abs < 86_400 ? { n: Math.floor(abs / 3_600), label: "hour" }
    : abs < 2_592_000 ? { n: Math.floor(abs / 86_400), label: "day" }
    : abs < 31_536_000 ? { n: Math.floor(abs / 2_592_000), label: "month" }
    : { n: Math.floor(abs / 31_536_000), label: "year" };

  const noun = unit.n === 1 ? unit.label : `${unit.label}s`;
  return past ? `${unit.n} ${noun} ago` : `in ${unit.n} ${noun}`;
}

function visibleLabel(
  value: string | Date,
  format: TimeFormat,
  locale: string,
  iso: string,
): string {
  if (format === "datetime") return iso;
  if (format === "relative") return formatTimeRelative(value);
  return formatTimeAbsolute(value, locale);
}

export function Time({
  value,
  format = "absolute",
  tooltip,
  locale = DEFAULT_LOCALE,
  label: labelOverride,
  className,
  ref,
  ...rest
}: TimeProps) {
  const d = coerceDate(value);
  const iso = toIso(d);
  const label = labelOverride ?? visibleLabel(value, format, locale, iso);
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

  const tooltipBody = formatTimeAbsolute(value, locale);
  return (
    <Tooltip>
      <TooltipTrigger asChild>{timeEl}</TooltipTrigger>
      <TooltipContent>{tooltipBody}</TooltipContent>
    </Tooltip>
  );
}

export default Time;
