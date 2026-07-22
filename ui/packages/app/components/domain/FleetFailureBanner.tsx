"use client";

import { Alert, Time, cn } from "@agentsfleet/design-system";

import type { FailureBanner } from "@/lib/events/event-banner";

// One line above the conversation for a fleet that keeps failing the same way.
// It exists so the operator does not have to read fifteen rows to learn one
// fact, and it says only what those rows already say — what broke, why, how
// often, and when it last happened.

const COUNT_PREFIX = "×";
const LAST_SEEN_LABEL = "last";
const SEPARATOR = "·";

export function FleetFailureBanner({ banner }: { banner: FailureBanner | null }) {
  // Nothing pinned means the fleet is not currently failing — the absence is
  // the signal, so there is no placeholder to render.
  if (banner === null) return null;
  return (
    <Alert
      variant="destructive"
      data-testid="fleet-failure-banner"
      className={cn(
        "flex flex-wrap items-baseline gap-sm rounded-none border-x-0 border-t-0",
        "px-xl py-md font-mono text-label leading-mono",
      )}
    >
      <span className="font-medium text-foreground">{banner.sentence}</span>
      <span className="tabular-nums text-foreground" data-testid="failure-banner-count">
        {COUNT_PREFIX}
        {banner.count}
      </span>
      {banner.detail ? (
        <>
          <span aria-hidden="true">{SEPARATOR}</span>
          <span className="min-w-0">{banner.detail}</span>
        </>
      ) : null}
      <span aria-hidden="true">{SEPARATOR}</span>
      <span className="shrink-0">
        {LAST_SEEN_LABEL}{" "}
        <Time value={banner.lastSeen} format="clock" className="font-mono text-label tabular-nums" />
      </span>
      {banner.guidance ? (
        <>
          <span aria-hidden="true">{SEPARATOR}</span>
          <span className="min-w-0">{banner.guidance}</span>
        </>
      ) : null}
    </Alert>
  );
}
