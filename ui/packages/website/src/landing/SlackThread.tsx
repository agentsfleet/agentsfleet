import type { ReactNode } from "react";
import { BrandMark, Card } from "@agentsfleet/design-system";

export function SlackThread({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <Card
      asChild
      className="flex h-full flex-col overflow-hidden border-zinc-700 bg-zinc-900 p-0 text-zinc-100"
    >
      <section aria-label={label}>
        <div className="flex flex-wrap items-center justify-between gap-sm border-b border-zinc-700 px-xl py-lg text-body-sm">
          <strong className="font-semibold">
            <span className="mr-sm text-zinc-400" aria-hidden="true">
              #
            </span>
            customer-support
          </strong>
          <span className="text-label text-zinc-300">Slack thread</span>
        </div>
        <div className="grid flex-1 content-start gap-lg p-xl sm:content-center sm:p-2xl">
          {children}
        </div>
      </section>
    </Card>
  );
}

export function SlackMessage({
  bot = false,
  time,
  children,
}: {
  bot?: boolean;
  time: string;
  children: ReactNode;
}) {
  return (
    <div className="grid grid-cols-[2rem_minmax(0,1fr)] gap-lg">
      <span
        aria-hidden="true"
        className="grid size-8 place-items-center rounded-sm bg-zinc-800 font-bold text-zinc-100"
      >
        {bot ? <BrandMark className="size-5 text-emerald-200" /> : "L"}
      </span>
      <div>
        <p className="flex flex-wrap items-center gap-md text-label">
          <strong>{bot ? "agentfleet" : "Leah"}</strong>
          {bot && (
            <small className="rounded-xs bg-zinc-700 px-1 text-xs font-semibold">
              APP
            </small>
          )}
          <time className="text-zinc-300">{time}</time>
        </p>
        <p className="mt-1 text-base/relaxed text-zinc-200 sm:text-sm/relaxed">
          {children}
        </p>
      </div>
    </div>
  );
}

export function Mention() {
  return (
    <b className="rounded-xs bg-sky-950 px-1 font-semibold text-sky-200">
      @agentfleet
    </b>
  );
}
