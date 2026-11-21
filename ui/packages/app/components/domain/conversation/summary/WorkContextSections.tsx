import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskDetails } from "../../tasks/use-task-details";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { ArrowUpRightIcon, CaretDownIcon, GitBranchIcon, GitDiffIcon, GitPullRequestIcon } from "@phosphor-icons/react/ssr";
import { confirmed } from "../../tasks/use-task-read";
import { FleetApprovalSummary } from "../../approvals/FleetApprovalSummary";
import { AttachedSources, PreviewOutputs } from "./RunResources";
import { WorkSection } from "../../shared/summary/WorkSection";
import type { RecordedChange, RecordedChangeRepository } from "./recorded-changes";
import { RecordedChangeCounts, RecordedChangesDialog } from "./RecordedChangesDialog";
import { groupRecordedChanges, recordedChanges } from "./recorded-changes";



export function WorkOutputs({details, expectedDetails = details.size, hideEmpty = false, previews = []}: {previews?: readonly string[]; details: TaskDetails; expectedDetails?: number; hideEmpty?: boolean}) {
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
    {previews.length || published.length ? <WorkSection title="Output"><PreviewOutputs links={previews} /><PullRequestRows records={published} /></WorkSection> : loading || unavailable || !hideEmpty && !related.length
      ? <WorkSection title="Output"><p className="text-eyebrow leading-eyebrow text-muted-foreground">{loading ? "Loading outputs…" : unavailable ? "Outputs unavailable." : "No outputs yet."}</p></WorkSection> : null}
    {related.length ? <WorkSection title="Pull requests"><PullRequestRows records={related} /></WorkSection> : null}
  </>;
}

type PullRequestRecord = {target: {url: string; number: number; repository: string}; published: boolean; stale: boolean};
function PullRequestRows({records}: {records: PullRequestRecord[]}) {
  return <ul className="space-y-sm">{records.map(({target, published, stale}) => <li key={target.url}>
    <a className="flex min-h-9 items-center gap-md py-sm break-words text-label font-medium leading-label hover:underline" href={target.url} target="_blank" rel="noopener noreferrer">
      <GitPullRequestIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span className="min-w-0 flex-1"><span className="block">Pull request #{target.number}</span>
        <span className="mt-xs block text-eyebrow font-normal leading-eyebrow text-text-dim">{published ? "Changes published" : target.repository}{stale ? " · last confirmed" : ""}</span></span>
      <ArrowUpRightIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /></a>
  </li>)}</ul>;
}


export function WorkChanges({events, repositories}: {events: readonly FleetEvent[]; repositories?: readonly RecordedChangeRepository[]}) {
  return <ChangesSummary changes={recordedChanges(events, {repositories})} />;
}

/** Explicit repository metadata is required for repository labels. */
export function ChangesSummary({changes}: {changes: readonly RecordedChange[]}) {
  if (!changes.length) return null;
  return <WorkSection title="Changes">{groupRecordedChanges(changes).map(group => <RecordedChangesDialog key={group.repository?.id ?? "unbound"} changes={group.changes}>
    <Button variant="ghost" size="compact" className="h-auto min-h-11 w-full justify-start gap-md whitespace-normal px-0 py-sm text-left text-foreground summary:min-h-9">
      <GitDiffIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span className="min-w-0 flex-1" title={group.repository?.fullName}><span className="truncate font-medium summary:block">{group.label}</span>
        <span className="ml-sm text-eyebrow font-normal leading-eyebrow text-muted-foreground summary:ml-0 summary:mt-0.5 summary:block">{group.changes.length} {group.changes.length === 1 ? "file" : "files"}</span></span>
      {group.lines.kind !== "unknown" ? <RecordedChangeCounts lines={group.lines} /> : null}
    </Button>
  </RecordedChangesDialog>)}</WorkSection>;
}

export function WorkSources({sources, files}: {sources?: FleetSources; files?: readonly {id: string; name: string}[]}) {
  return <WorkSection title="Sources">
      {files?.length ? <AttachedSources files={files} /> : null}
      {sources?.repositories.length ? <details className="group">
        <summary className="flex min-h-11 cursor-pointer items-center gap-sm rounded-sm py-sm text-label leading-label focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring summary:min-h-9">
          <GitBranchIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" /><span className="min-w-0 flex-1 break-words" title={sources.repositories.join(", ")}><span className="block font-medium">{sources.repositories.length === 1 ? sources.repositories[0] : `${sources.repositories.length} repositories`}</span><span className="mt-xs block text-eyebrow leading-eyebrow text-muted-foreground">Saved fleet policy{sources.access ? ` · ${sources.access} access` : " · access not specified"}{sources.publication ? ` · ${sources.publication === "existing_pr" ? "existing PRs" : "draft PRs"}` : ""}</span></span>
          <CaretDownIcon size={16} className="shrink-0 text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" />
        </summary>
        <ul className="mt-sm space-y-sm">{sources.repositories.map(repository => <li key={repository} className="break-words text-label leading-label"><a href={`https://github.com/${repository}`} target="_blank" rel="noopener noreferrer" className="hover:underline">{repository}</a></li>)}</ul>
      </details> : files?.length ? null : <p className="text-eyebrow leading-eyebrow text-muted-foreground">No sources configured.</p>}
  </WorkSection>;
}

export function WorkApprovals({approvals, scope}: {approvals?: {count: number; href: string}; scope?: {workspaceId: string; fleetId: string; subject: string; revision: number}}) {
  if (scope && approvals) return <FleetApprovalSummary key={`${scope.subject}:${scope.workspaceId}:${scope.fleetId}`} workspaceId={scope.workspaceId} fleetId={scope.fleetId} revision={scope.revision} count={approvals.count} />;
  if (!approvals?.count) return null;
  return <WorkSection title="Needs your approval" count={approvals.count} attention>
    <Button asChild size="sm" className="min-h-11 summary:min-h-10"><Link href={approvals.href} aria-label={`Open fleet with ${approvals.count} pending ${approvals.count === 1 ? "approval" : "approvals"}`}>Open fleet</Link></Button>
  </WorkSection>;
}
