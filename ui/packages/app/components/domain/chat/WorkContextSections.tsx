import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskDetails } from "./use-task-details";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { ArrowUpRightIcon, CaretDownIcon, FileTextIcon, GitBranchIcon, GitDiffIcon, GitPullRequestIcon } from "@phosphor-icons/react/ssr";
import { TOOL_CALL_STATUS } from "@/lib/streaming/fleet-stream-tool-trace";
import { TOOL_NAME } from "../tool-call-shape";
import { confirmed } from "./use-task-read";
import { FleetApprovalSummary } from "./FleetApprovalSummary";
import { WorkSection } from "./WorkSection";
export { WorkSection } from "./WorkSection";

const WRITES: ReadonlySet<string> = new Set([TOOL_NAME.FILE_WRITE, TOOL_NAME.FILE_EDIT, TOOL_NAME.FILE_EDIT_HASHED, TOOL_NAME.FILE_APPEND, TOOL_NAME.FILE_DELETE]);


export function WorkOutputs({details, expectedDetails = details.size, hideEmpty = false}: {details: TaskDetails; expectedDetails?: number; hideEmpty?: boolean}) {
  const records = [...details.values()].flatMap(resource => {
    const detail = confirmed(resource);
    const target = detail?.context.target;
    if (!detail || target?.kind !== "existing_pr") return [];
    const receipt = detail.publication;
    const published = receipt?.state === "confirmed" && receipt.commit !== null && receipt.pull_request_url === target.url;
    return [{target, published, stale: resource.kind !== "ready"}];
  });
  const published = [...new Map(records.filter(row => row.published).map(row => [row.target.url, row])).values()];
  const related = [...new Map(records.filter(row => !row.published && !published.some(output => output.target.url === row.target.url)).map(row => [row.target.url, row])).values()];
  const loading = details.size < expectedDetails || [...details.values()].some(resource => resource.kind === "loading" && !confirmed(resource));
  const unavailable = [...details.values()].some(resource => resource.kind === "failed" && !confirmed(resource));
  return <>
    {published.length ? <PullRequestSection title="Outputs" records={published} /> : loading || unavailable || !hideEmpty && !related.length
      ? <WorkSection title="Outputs"><p className="text-eyebrow leading-eyebrow text-muted-foreground">{loading ? "Loading outputs…" : unavailable ? "Outputs unavailable." : "No outputs yet."}</p></WorkSection> : null}
    {related.length ? <PullRequestSection title="Pull requests" records={related} /> : null}
  </>;
}

type PullRequestRecord = {target: {url: string; number: number; repository: string}; published: boolean; stale: boolean};
function PullRequestSection({title, records}: {title: string; records: PullRequestRecord[]}) {
  return <WorkSection title={title}><ul className="space-y-sm">{records.map(({target, published, stale}) => <li key={target.url}>
    <a className="flex min-h-9 items-center gap-md py-sm break-words text-label font-medium leading-label hover:underline" href={target.url} target="_blank" rel="noopener noreferrer">
      <GitPullRequestIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span className="min-w-0 flex-1"><span className="block">Pull request #{target.number}</span>
        <span className="mt-xs block text-eyebrow font-normal leading-label text-text-dim">{published ? "Changes published" : target.repository}{stale ? " · last confirmed" : ""}</span></span>
      <ArrowUpRightIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /></a>
  </li>)}</ul></WorkSection>;
}

export function recordedChanges(events: readonly FleetEvent[], {includeRunning = true}: {includeRunning?: boolean} = {}): {path: string; running: boolean}[] {
  const paths = new Map<string, {path: string; running: boolean}>();
  for (const event of events) for (const tool of event.tools ?? []) {
    const path = tool.args?.path;
    if (!WRITES.has(tool.name) || typeof path !== "string" || !path.trim()) continue;
    const running = !tool.done && !tool.status;
    if (running && !includeRunning) continue;
    if (!running && tool.status !== TOOL_CALL_STATUS.SUCCEEDED) continue;
    paths.delete(path);
    paths.set(path, {path, running});
  }
  return [...paths.values()].reverse();
}

export function WorkChanges({events}: {events: readonly FleetEvent[]}) {
  const changes = recordedChanges(events);
  if (!changes.length) return null;
  return <WorkSection title="Changes">
    <details className="group"><summary className="flex min-h-11 cursor-pointer items-center gap-sm rounded-sm text-label focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring summary:min-h-9"><GitDiffIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="min-w-0 flex-1 font-medium">{changes.length} {changes.length === 1 ? "file" : "files"}</span><CaretDownIcon size={16} className="shrink-0 text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" /></summary>
      <ul className="mt-sm space-y-sm">{changes.map(change => <li key={change.path} className="break-all text-eyebrow"><span className="flex items-start gap-sm"><FileTextIcon size={16} className={`mt-xs shrink-0 ${change.running ? "text-info" : "text-success"}`} aria-hidden="true" />{change.path}</span><span className="block text-muted-foreground">{change.running ? "Write in progress" : "Local write recorded"}</span></li>)}</ul>
    </details>
  </WorkSection>;
}

export function WorkSources({sources}: {sources?: FleetSources}) {
  return <WorkSection title="Sources">
      {sources?.repositories.length ? <details className="group">
        <summary className="flex min-h-11 cursor-pointer items-center gap-sm rounded-sm text-label leading-label focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring summary:min-h-9">
          <GitBranchIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="min-w-0 flex-1 break-words" title={sources.repositories.join(", ")}><span className="block">{sources.repositories.length === 1 ? sources.repositories[0] : `${sources.repositories.length} repositories`}</span><span className="mt-xs block text-eyebrow text-muted-foreground">Saved fleet policy{sources.access ? ` · ${sources.access} access` : " · access not specified"}{sources.publication ? ` · ${sources.publication === "existing_pr" ? "existing PRs" : "draft PRs"}` : ""}</span></span>
          <CaretDownIcon size={16} className="shrink-0 text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" />
        </summary>
        <ul className="mt-sm space-y-sm">{sources.repositories.map(repository => <li key={repository} className="break-words text-label leading-label"><a href={`https://github.com/${repository}`} target="_blank" rel="noopener noreferrer" className="hover:underline">{repository}</a></li>)}</ul>
      </details> : <p className="text-eyebrow leading-eyebrow text-muted-foreground">No sources configured.</p>}
  </WorkSection>;
}

export function WorkApprovals({approvals, scope}: {approvals?: {count: number; href: string}; scope?: {workspaceId: string; fleetId: string; subject: string; revision: number}}) {
  if (scope && approvals) return <FleetApprovalSummary key={`${scope.subject}:${scope.workspaceId}:${scope.fleetId}`} workspaceId={scope.workspaceId} fleetId={scope.fleetId} revision={scope.revision} count={approvals.count} />;
  if (!approvals?.count) return null;
  return <WorkSection title="Needs your approval" count={approvals.count} attention>
    <Button asChild size="sm" className="min-h-11 summary:min-h-10"><Link href={approvals.href} aria-label={`Open fleet with ${approvals.count} pending ${approvals.count === 1 ? "approval" : "approvals"}`}>Open fleet</Link></Button>
  </WorkSection>;
}
