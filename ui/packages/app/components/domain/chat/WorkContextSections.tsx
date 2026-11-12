import type { ReactNode } from "react";
import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskDetails } from "./use-task-details";
import Link from "next/link";
import { Badge, Button, Section, cn } from "@agentsfleet/design-system";
import { CaretDownIcon, FileTextIcon, GitBranchIcon, GitPullRequestIcon, ShieldCheckIcon } from "@phosphor-icons/react/ssr";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_NAME } from "../tool-call-shape";
import { confirmed } from "./use-task-read";

const WRITES: ReadonlySet<string> = new Set([TOOL_NAME.FILE_WRITE, TOOL_NAME.FILE_EDIT, TOOL_NAME.FILE_EDIT_HASHED, TOOL_NAME.FILE_APPEND, TOOL_NAME.FILE_DELETE]);

export function WorkSection({title, children, count, attention = false}: {title: string; children: ReactNode; count?: number; attention?: boolean}) {
  return <Section asChild className="block gap-0"><section aria-label={title} className="min-w-0 space-y-sm border-t border-border pt-md first:border-0 first:pt-0">
    <h2 className={cn("flex items-center justify-between gap-md text-eyebrow leading-eyebrow font-medium", attention ? "text-warning" : "text-muted-foreground")}>{title}{count === undefined ? null : <Badge className="h-5.5 min-w-5.5 justify-center border-0 p-0 text-eyebrow leading-eyebrow text-current">{count}</Badge>}</h2>{children}
  </section></Section>;
}

export function WorkOutputs({details, expectedDetails = details.size}: {details: TaskDetails; expectedDetails?: number}) {
  const outputs = [...new Map([...details.values()].flatMap(resource => {
    const detail = confirmed(resource);
    const target = detail?.context.target;
    if (!detail || target?.kind !== "existing_pr") return [];
    return [[target.url, {target, stale: resource.kind !== "ready"}] as const];
  })).values()];
  const loading = details.size < expectedDetails || [...details.values()].some(resource => resource.kind === "loading" && !confirmed(resource));
  const unavailable = [...details.values()].some(resource => resource.kind === "failed" && !confirmed(resource));
  return <WorkSection title="Outputs">
    {outputs.length ? <ul className="space-y-sm">{outputs.map(({target, stale}) => <li key={target.url}>
      <a className="flex items-start gap-sm break-words text-label leading-label hover:underline" href={target.url} target="_blank" rel="noopener noreferrer"><GitPullRequestIcon size={16} className="mt-xs shrink-0 text-muted-foreground" aria-hidden="true" /><span>Pull request #{target.number}</span></a>
      <p className="text-eyebrow text-muted-foreground">{target.repository}{stale ? " · last confirmed" : ""}</p>
    </li>)}</ul> : <p className="text-eyebrow leading-eyebrow text-muted-foreground">{loading ? "Loading outputs…" : unavailable ? "Outputs unavailable." : "No outputs yet."}</p>}
  </WorkSection>;
}

export function recordedChanges(events: readonly FleetEvent[]): {path: string; running: boolean}[] {
  const paths = new Map<string, {path: string; running: boolean}>();
  for (const event of events) for (const tool of event.tools ?? []) {
    const path = tool.args?.path;
    if (!WRITES.has(tool.name) || typeof path !== "string" || !path.trim()) continue;
    const running = !tool.done && !tool.status;
    if (!running && tool.status !== TOOL_CALL_STATUS.SUCCEEDED) continue;
    paths.delete(path);
    paths.set(path, {path, running});
  }
  return [...paths.values()].reverse();
}

export function WorkChanges({events}: {events: readonly FleetEvent[]}) {
  const changes = recordedChanges(events);
  return <WorkSection title="Changes">
    {changes.length ? <details className="group"><summary className="flex min-h-11 cursor-pointer items-center gap-sm rounded-sm text-label focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring summary:min-h-9"><GitBranchIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="min-w-0 flex-1">{changes.length} {changes.length === 1 ? "file" : "files"}</span><CaretDownIcon size={16} className="shrink-0 text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" /></summary>
      <ul className="mt-sm space-y-sm">{changes.map(change => <li key={change.path} className="break-all text-eyebrow"><span className="flex items-start gap-sm"><FileTextIcon size={16} className={`mt-xs shrink-0 ${change.running ? "text-info" : "text-success"}`} aria-hidden="true" />{change.path}</span><span className="block text-muted-foreground">{change.running ? "Write in progress" : "Local write recorded"}</span></li>)}</ul>
    </details> : <p className="text-eyebrow leading-eyebrow text-muted-foreground">No changes yet.</p>}
  </WorkSection>;
}

export function WorkSources({sources}: {sources?: FleetSources}) {
  return <WorkSection title="Sources">
      {sources?.repositories.length ? <details className="group">
        <summary className="flex min-h-11 cursor-pointer items-center gap-sm rounded-sm text-label leading-label focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring summary:min-h-9">
          <GitBranchIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="min-w-0 flex-1 break-words" title={sources.repositories.join(", ")}>{sources.repositories.length === 1 ? sources.repositories[0] : `${sources.repositories.length} repositories`}</span>
          <CaretDownIcon size={16} className="shrink-0 text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" />
        </summary>
        <ul className="mt-sm space-y-sm">{sources.repositories.map(repository => <li key={repository} className="break-words text-label leading-label"><a href={`https://github.com/${repository}`} target="_blank" rel="noopener noreferrer" className="hover:underline">{repository}</a></li>)}</ul>
        <p className="mt-sm flex items-start gap-xs text-eyebrow text-muted-foreground"><ShieldCheckIcon size={16} className="shrink-0 text-evidence" aria-hidden="true" /><span>Saved fleet policy{sources.access ? ` · ${sources.access} access` : " · access not specified"}{sources.publication ? ` · ${sources.publication === "existing_pr" ? "existing PRs" : "draft PRs"}` : ""}</span></p>
      </details> : <p className="text-eyebrow leading-eyebrow text-muted-foreground">No sources configured.</p>}
  </WorkSection>;
}

export function WorkApprovals({approvals}: {approvals?: {count: number; href: string}}) {
  if (!approvals?.count) return null;
  return <WorkSection title="Needs your approval" count={approvals.count} attention>
    <Button asChild size="sm" className="min-h-11 summary:min-h-9"><Link href={approvals.href} aria-label={`Review ${approvals.count} pending ${approvals.count === 1 ? "approval" : "approvals"}`}>Review</Link></Button>
  </WorkSection>;
}
