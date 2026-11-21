import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import type { TaskDetails } from "../../tasks/use-task-details";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { ArrowUpRightIcon, CaretDownIcon, CaretRightIcon, GitBranchIcon, GitDiffIcon, GitPullRequestIcon } from "@phosphor-icons/react/ssr";
import { confirmed } from "../../tasks/use-task-read";
import { FleetApprovalSummary } from "../../approvals/FleetApprovalSummary";
import { AttachedSources, PreviewOutputs } from "./RunResources";
import { WorkSection } from "../../shared/summary/WorkSection";
import type { RecordedChange, RecordedChangeRepository } from "./recorded-changes";
import { RecordedChangeCounts, RecordedChangesDialog } from "./RecordedChangesDialog";
import { groupRecordedChanges, recordedChanges } from "./recorded-changes";


export function WorkOutputs({details, expectedDetails = details.size, hideEmpty = false, previews = []}: {previews?: readonly string[]; details: TaskDetails; expectedDetails?: number; hideEmpty?: boolean}) {
  const published = new Map<string, PullRequestRecord>();
  const related = new Map<string, PullRequestRecord>();
  let loading = details.size < expectedDetails;
  let unavailable = false;
  for (const resource of details.values()) {
    const detail = confirmed(resource);
    if (!detail) {
      loading ||= resource.kind === "loading";
      unavailable ||= resource.kind === "failed";
      continue;
    }
    const {target} = detail.context;
    if (target?.kind !== "existing_pr") continue;
    const receipt = detail.publication;
    const delivered = receipt?.state === "confirmed" && receipt.commit !== null && receipt.pull_request_url === target.url;
    const records = delivered ? published : related;
    records.set(target.url, {target, published: delivered, stale: resource.kind !== "ready"});
  }
  for (const url of published.keys()) related.delete(url);

  const hasOutput = previews.length > 0 || published.size > 0;
  const showOutput = hasOutput || loading || unavailable || (!hideEmpty && !related.size);
  return <>
    {showOutput && <WorkSection title="Output">
      {hasOutput ? <><PreviewOutputs links={previews} /><PullRequestRows records={[...published.values()]} /></> :
        <p className="text-eyebrow leading-eyebrow text-muted-foreground">{loading ? "Loading outputs…" : unavailable ? "Outputs unavailable." : "No outputs yet."}</p>}
    </WorkSection>}
    {related.size > 0 && <WorkSection title="Pull requests"><PullRequestRows records={[...related.values()]} /></WorkSection>}
  </>;
}

type PullRequestRecord = {target: {url: string; number: number; repository: string}; published: boolean; stale: boolean};
function PullRequestRows({records}: {records: PullRequestRecord[]}) {
  return <ul className="space-y-sm">{records.map(({target, published, stale}) => <li key={target.url}>
    <a className="fleet-pull-request-link" href={target.url} target="_blank" rel="noopener noreferrer">
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
  return <WorkSection title="Changes">{groupRecordedChanges(changes).map(group => <RecordedChangesDialog key={group.repository?.id ?? "unbound"} groups={[group]}>
    <Button variant="ghost" size="compact" className="fleet-changes-trigger">
      <GitDiffIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span className="min-w-0 flex-1" title={group.repository?.fullName}>
        <span className="block font-medium">{group.changes.length} {group.changes.length === 1 ? "file" : "files"}{group.changes.some(change => change.running) ? "" : " changed"}</span>
        {group.label ? <RepositoryName name={group.label} className="text-eyebrow font-normal text-muted-foreground" /> : null}
      </span>
      {group.lines.kind !== "unknown" ? <RecordedChangeCounts lines={group.lines} /> : null}
      <CaretRightIcon size={14} className="shrink-0 text-muted-foreground" aria-hidden="true" />
    </Button>
  </RecordedChangesDialog>)}</WorkSection>;
}

function RepositoryName({name, className = "font-medium"}: {name: string; className?: string}) {
  return <span className={`repo-name min-w-0 ${className}`}><span>{name}</span></span>;
}

export function WorkSources({sources, files}: {sources?: FleetSources; files?: readonly {id: string; name: string}[]}) {
  return <WorkSection title="Sources">
    {files?.length ? <AttachedSources files={files} /> : null}
    {sources?.repositories.length ? <RepositorySources sources={sources} /> : files?.length ? null :
      <p className="text-eyebrow leading-eyebrow text-muted-foreground">No sources configured.</p>}
  </WorkSection>;
}

function RepositorySources({sources}: {sources: FleetSources}) {
  const {repositories, access, publication} = sources;
  const label = repositories.length === 1 ? repositories[0] ?? "" : `${repositories.length} repositories`;
  const policy = ["Saved fleet policy", access ? `${access} access` : "access not specified"];
  if (publication) policy.push(publication === "existing_pr" ? "existing PRs" : "draft PRs");
  return <details className="group">
    <summary className="fleet-sources-trigger">
      <GitBranchIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span className="min-w-0 flex-1 break-words" title={repositories.join(", ")}>
        <RepositoryName name={label} />
        <span className="mt-xs block text-eyebrow leading-eyebrow text-muted-foreground">{policy.join(" · ")}</span>
      </span>
      <CaretDownIcon size={16} className="shrink-0 text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" />
    </summary>
    <ul className="mt-sm space-y-sm">{repositories.map(repository => <li key={repository} className="break-words text-label leading-label">
      <a href={`https://github.com/${repository}`} target="_blank" rel="noopener noreferrer" className="hover:underline">{repository}</a>
    </li>)}</ul>
  </details>;
}

export function WorkApprovals({approvals, scope}: {approvals?: {count: number; href: string}; scope?: {workspaceId: string; fleetId: string; subject: string; revision: number}}) {
  if (scope && approvals) return <FleetApprovalSummary key={`${scope.subject}:${scope.workspaceId}:${scope.fleetId}`} workspaceId={scope.workspaceId} fleetId={scope.fleetId} revision={scope.revision} count={approvals.count} />;
  if (!approvals?.count) return null;
  return <WorkSection title="Needs your approval" count={approvals.count} attention>
    <Button asChild size="sm" className="min-h-11 summary:min-h-10"><Link href={approvals.href} aria-label={`Open fleet with ${approvals.count} pending ${approvals.count === 1 ? "approval" : "approvals"}`}>Open fleet</Link></Button>
  </WorkSection>;
}
