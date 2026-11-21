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
        <summary className="flex min-h-11 w-fit max-w-full cursor-pointer list-none items-center gap-2 rounded-md px-2 text-label leading-5 text-muted-foreground hover:bg-secondary hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring md:min-h-9 pointer-coarse:min-h-11 [&::-webkit-details-marker]:hidden">
          <Icon aria-hidden className={cn("size-4 shrink-0", kind === "agent" && !outcome && "text-[var(--activity-agent)]", outcome === "failed" && "text-error", outcome === "interrupted" && "text-warning")} /><span className="min-w-0 break-words">{label}</span>
          <CaretRightIcon aria-hidden className="size-3 shrink-0 opacity-0 group-open/trace:rotate-90 group-open/trace:opacity-100 group-focus-within/trace:opacity-100 group-hover/trace:opacity-100" />
        </summary>
        <p className="mb-2 ml-4 whitespace-pre-wrap break-words border-l border-border pl-3 text-label leading-5 text-muted-foreground">{detail}</p>
      </details></li>;
    })}
  </ul>;
}
