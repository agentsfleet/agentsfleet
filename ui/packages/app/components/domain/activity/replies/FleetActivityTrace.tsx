import { CaretRightIcon, CheckCircleIcon, FileTextIcon, ImagesIcon, PencilSimpleIcon, PlugsConnectedIcon, TerminalWindowIcon, UsersIcon } from "@phosphor-icons/react/ssr";
import { cn } from "@agentsfleet/design-system";
import { readActivityTrace } from "./fleet-activity-trace";

const ICONS = {files: FileTextIcon, images: ImagesIcon, agent: UsersIcon, edits: PencilSimpleIcon,
  integration: PlugsConnectedIcon, process: TerminalWindowIcon, check: CheckCircleIcon};

export function FleetActivityTrace({data}: {data: unknown}) {
  const rows = readActivityTrace(data);
  if (!rows.length) return <p className="text-label text-muted-foreground">Activity details · Preview unavailable.</p>;
  return <ul aria-label="Work activity" className="min-w-0">
    {rows.map(({kind, label, detail, outcome}, index) => {
      const Icon = ICONS[kind];
      return <li key={`${kind}-${index}`}><details className="group/trace">
        <summary className="fleet-trace-action">
          <Icon aria-hidden className={cn("size-4 shrink-0", kind === "agent" && !outcome && "text-activity-agent", outcome === "failed" && "text-error", outcome === "interrupted" && "text-warning")} /><span className="min-w-0 break-words font-medium">{label}</span>
          <CaretRightIcon aria-hidden className="fleet-trace-caret" />
        </summary>
        <p className="fleet-trace-detail">{detail}</p>
      </details></li>;
    })}
  </ul>;
}
