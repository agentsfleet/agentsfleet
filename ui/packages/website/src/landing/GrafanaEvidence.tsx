import type { ReactNode } from "react";
import { CaretDownIcon } from "@phosphor-icons/react/dist/ssr/CaretDown";
import { CheckCircleIcon } from "@phosphor-icons/react/dist/ssr/CheckCircle";

const queries = [
  {
    name: "Read Grafana metrics",
    output:
      "Prometheus · 08:00–09:00 IST\n95th-percentile reply start: 15.1 s\nPacket loss: 8% · Runner reconnects: 5\nThe model answered as fast as usual.",
  },
  {
    name: "Read Grafana logs",
    output:
      "Loki · Runner connection events\nThe runner lost its connection during the freezes.",
  },
] as const;

function Activity({
  label,
  children,
  onInspect,
}: {
  label: string;
  children: ReactNode;
  onInspect: () => void;
}) {
  return (
    <details className="min-w-0">
      <summary
        onClick={onInspect}
        className="flex min-h-11 cursor-pointer list-none items-center gap-md rounded-sm px-md text-label text-zinc-200 hover:bg-zinc-800 focus-visible:outline-2 focus-visible:outline-emerald-200 [&::-webkit-details-marker]:hidden"
      >
        <CheckCircleIcon
          aria-hidden="true"
          className="size-4 shrink-0 text-emerald-200"
        />
        <span className="flex-1">{label}</span>
        <CaretDownIcon
          aria-hidden="true"
          className="size-4 shrink-0 [[open]>summary>&]:rotate-180"
        />
      </summary>
      {children}
    </details>
  );
}

export function GrafanaEvidence({ onInspect }: { onInspect: () => void }) {
  return (
    <Activity label="Worked for 4s · 2 tools" onInspect={onInspect}>
      <div className="ml-lg border-l border-zinc-700 pl-sm">
        {queries.map(({ name, output }) => (
          <Activity key={name} label={name} onInspect={onInspect}>
            <pre className="mx-md mb-md rounded-sm bg-zinc-950 p-md font-mono text-xs/relaxed whitespace-pre-wrap text-zinc-200">
              <code>{output}</code>
            </pre>
          </Activity>
        ))}
      </div>
    </Activity>
  );
}
