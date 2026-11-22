import { BrandMark, Card } from "@agentsfleet/design-system";
import { ArrowUpIcon } from "@phosphor-icons/react/dist/ssr/ArrowUp";
import { AtIcon } from "@phosphor-icons/react/dist/ssr/At";
import { CpuIcon } from "@phosphor-icons/react/dist/ssr/Cpu";
import { KeyIcon } from "@phosphor-icons/react/dist/ssr/Key";
import { NotePencilIcon } from "@phosphor-icons/react/dist/ssr/NotePencil";
import { PaperclipIcon } from "@phosphor-icons/react/dist/ssr/Paperclip";
import { PlugsConnectedIcon } from "@phosphor-icons/react/dist/ssr/PlugsConnected";
import { StackIcon } from "@phosphor-icons/react/dist/ssr/Stack";
import { FleetSummary } from "./FleetSummary";
import { GrafanaEvidence } from "./GrafanaEvidence";

const railIcons = [NotePencilIcon, KeyIcon, PlugsConnectedIcon, StackIcon];

function FleetRail() {
  return (
    <div
      aria-hidden="true"
      className="hidden w-11 shrink-0 flex-col items-center gap-lg border-r border-zinc-700 bg-zinc-950 py-lg text-zinc-300 sm:flex"
    >
      <BrandMark className="mb-sm size-6 text-emerald-200" />
      {railIcons.map((Icon, index) => (
        <Icon key={index} className="size-4" />
      ))}
    </div>
  );
}

function ComposerPreview() {
  return (
    <div
      aria-hidden="true"
      className="mt-auto rounded-md border border-zinc-600 p-md text-label text-zinc-300"
    >
      <div>Message your fleet…</div>
      <div className="mt-md flex items-center gap-lg">
        <PaperclipIcon className="size-4 shrink-0" />
        <AtIcon className="size-4 shrink-0" />
        <span className="flex items-center gap-sm">
          <CpuIcon className="size-4 shrink-0" /> Default model
        </span>
        <ArrowUpIcon className="ml-auto size-6 shrink-0 rounded-full bg-zinc-700 p-1" />
      </div>
    </div>
  );
}

export function ScoutInvestigation({ onInspect }: { onInspect: () => void }) {
  return (
    <Card
      className="flex h-full overflow-hidden border-zinc-700 bg-zinc-900 p-0 text-zinc-100"
      aria-label="Scout investigates the recurring chat freeze"
    >
      <FleetRail />
      <div className="@container flex min-w-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-md border-b border-zinc-700 px-lg py-lg text-label">
          <StackIcon
            aria-hidden="true"
            className="size-4 shrink-0 text-zinc-300"
          />
          <strong>Scout</strong>
          <span className="text-zinc-300">Performance check</span>
        </div>
        <div className="grid gap-lg p-lg lg:flex-1 @min-[30rem]:grid-cols-[minmax(0,1fr)_minmax(10rem,36%)]">
          <section
            className="flex min-w-0 flex-col gap-lg"
            aria-label="Scout’s conversation"
          >
            <div className="ml-auto max-w-[90%]">
              <p className="rounded-lg bg-zinc-800 px-lg py-md text-base/relaxed @min-[30rem]:text-label">
                What slowed down replies around 8:55?
              </p>
              <p className="mt-sm text-right text-xs text-zinc-300">
                @mira · 09:02
              </p>
            </div>
            <div className="grid gap-sm">
              <p className="flex items-center gap-md text-label">
                <BrandMark className="size-4 text-emerald-200" />{" "}
                <strong>Scout</strong>
                <time className="text-zinc-300">09:03</time>
              </p>
              <GrafanaEvidence onInspect={onInspect} />
              <p className="text-base/relaxed @min-[30rem]:text-label">
                <span className="block text-zinc-300">
                  Grafana · 08:00–09:00 IST
                </span>
                <strong>
                  About 1 in 20 replies took 15 seconds or more to start.
                </strong>{" "}
                The model answered as fast as usual.
              </p>
              <p className="text-base/relaxed @min-[30rem]:text-label">
                The runner lost <strong>8% of packets</strong> and reconnected{" "}
                <strong>5 times</strong> that hour.
              </p>
              <p className="text-label text-zinc-300">
                Scout used its saved Grafana credential for read-only queries.
              </p>
            </div>
            <ComposerPreview />
          </section>
          <FleetSummary onInspect={onInspect} />
        </div>
      </div>
    </Card>
  );
}
