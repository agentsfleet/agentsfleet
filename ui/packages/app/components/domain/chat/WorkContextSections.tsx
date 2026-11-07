import type { ReactNode } from "react";
import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskDetails } from "./use-task-details";
import { Section } from "@agentsfleet/design-system";
import { FolderGit2, GitPullRequest, ShieldCheck } from "lucide-react";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_NAME } from "../tool-call-shape";
import { confirmed } from "./use-task-read";

const WRITES: ReadonlySet<string> = new Set([TOOL_NAME.FILE_WRITE, TOOL_NAME.FILE_EDIT, TOOL_NAME.FILE_EDIT_HASHED, TOOL_NAME.FILE_APPEND, TOOL_NAME.FILE_DELETE]);

export function WorkSection({title, children}: {title: string; children: ReactNode}) {
  return <Section asChild className="block gap-0"><section aria-label={title} className="min-w-0 space-y-xs border-t border-border pt-md first:border-0 first:pt-0">
    <h2 className="text-body-sm font-medium text-muted-foreground">{title}</h2>{children}
  </section></Section>;
}

export function WorkOutputs({details}: {details: TaskDetails}) {
  const outputs = [...new Map([...details.values()].flatMap(resource => {
    const detail = confirmed(resource);
    const target = detail?.context.target;
    if (!detail || target?.kind !== "existing_pr") return [];
    return [[target.url, {target, title: detail.title, stale: resource.kind !== "ready"}] as const];
  })).values()];
  return <WorkSection title="Outputs">
    {outputs.length ? <ul className="space-y-sm">{outputs.map(({target, title, stale}) => <li key={target.url}>
      <a className="flex items-start gap-sm break-words text-eyebrow hover:underline" href={target.url} target="_blank" rel="noopener noreferrer"><GitPullRequest size={12} className="mt-xs shrink-0" aria-hidden="true" /><span>{title} <span className="text-muted-foreground">#{target.number}</span></span></a>
      <p className="text-eyebrow text-muted-foreground">{target.repository}{stale ? " · last confirmed" : ""}</p>
    </li>)}</ul> : <p className="text-eyebrow text-muted-foreground">No saved outputs.</p>}
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
    {changes.length ? <details><summary className="cursor-pointer text-eyebrow">{changes.length} recorded {changes.length === 1 ? "file" : "files"}</summary>
      <ul className="mt-sm space-y-sm">{changes.map(change => <li key={change.path} className="break-all text-eyebrow">{change.path}<span className="block text-muted-foreground">{change.running ? "Write in progress" : "Local write recorded"}</span></li>)}</ul>
    </details> : <p className="text-eyebrow text-muted-foreground">No recorded file changes.</p>}
  </WorkSection>;
}

export function WorkSources({sources}: {sources?: FleetSources}) {
  return <WorkSection title="Sources">
    {sources?.repositories.length ? <>
      <ul className="space-y-sm">{sources.repositories.map(repository => <li key={repository} className="break-words text-eyebrow"><a href={`https://github.com/${repository}`} target="_blank" rel="noopener noreferrer" className="flex items-center gap-sm hover:underline"><FolderGit2 size={12} className="shrink-0" aria-hidden="true" />{repository}</a></li>)}</ul>
      <details className="text-eyebrow text-muted-foreground"><summary className="cursor-pointer">Saved permissions</summary>
        <p className="mt-sm flex items-start gap-xs"><ShieldCheck size={12} className="shrink-0" aria-hidden="true" /><span>Saved fleet policy{sources.access ? ` · ${sources.access} access` : " · access not specified"}{sources.publication ? ` · ${sources.publication === "existing_pr" ? "existing PRs" : "draft PRs"}` : ""}</span></p>
      </details>
    </> : <p className="text-eyebrow text-muted-foreground">No saved repository scope.</p>}
  </WorkSection>;
}
