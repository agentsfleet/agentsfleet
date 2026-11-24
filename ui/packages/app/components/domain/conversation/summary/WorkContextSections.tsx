import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import type { DurableTaskDetails } from "../../tasks/use-task-details";
import Link from "next/link";
import { Button, List, ListItem, cn } from "@agentsfleet/design-system";
import { ArrowUpRightIcon, CaretDownIcon, CaretRightIcon, GitBranchIcon, GitDiffIcon, GitPullRequestIcon } from "@phosphor-icons/react/ssr";
import { SUMMARY_ICON, SUMMARY_ROW } from "@/components/domain/shared/summary/summary-row";
import { confirmed } from "../../tasks/use-task-read";
import { FleetApprovalSummary } from "../../approvals/FleetApprovalSummary";
import { AttachedSources, PreviewOutputs } from "./RunResources";
import { WorkSection } from "../../shared/summary/WorkSection";
import type { RecordedChange, RecordedChangeRepository } from "./recorded-changes";
import { RecordedChangeCounts, RecordedChangesDialog } from "./RecordedChangesDialog";
import { groupRecordedChanges, recordedChanges } from "./recorded-changes";


export function WorkOutputs({details, expectedDetails = details.size, hideEmpty = false, previews = []}: {previews?: readonly string[]; details: DurableTaskDetails; expectedDetails?: number; hideEmpty?: boolean}) {
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
  return <List variant="plain" className="space-y-sm text-label leading-label">{records.map(({target, published, stale}) => <ListItem key={target.url}>
    <a className={cn(SUMMARY_ROW, "min-h-9 break-words py-sm text-label font-medium leading-label hover:underline")} href={target.url} target="_blank" rel="noopener noreferrer">
      <GitPullRequestIcon className={cn(SUMMARY_ICON, "mt-xs self-start")} aria-hidden="true" />
      <span className="min-w-0 flex-1"><span className="block">Pull request #{target.number}</span>
        <span className="mt-xs block text-eyebrow font-normal leading-eyebrow text-text-dim">{published ? "Changes published" : target.repository}{stale ? " · last confirmed" : ""}</span></span>
      <ArrowUpRightIcon className={SUMMARY_ICON} aria-hidden="true" /></a>
  </ListItem>)}</List>;
}

export function WorkChanges({turns, repositories}: {turns: readonly FleetTurn[]; repositories?: readonly RecordedChangeRepository[]}) {
  return <ChangesSummary changes={recordedChanges(turns, {repositories})} />;
}

/** Explicit repository metadata is required for repository labels. */
export function ChangesSummary({changes}: {changes: readonly RecordedChange[]}) {
  if (!changes.length) return null;
  return <WorkSection title="Changes">{groupRecordedChanges(changes).map(group => <RecordedChangesDialog key={group.repository?.id ?? "unbound"} groups={[group]}>
    <Button variant="ghost" size="compact" className={cn(SUMMARY_ROW, "h-auto min-h-11 w-full justify-start whitespace-normal px-0 py-xs text-left text-foreground summary:min-h-9")}>
      <GitDiffIcon className={cn(SUMMARY_ICON, group.label && "mt-xs self-start")} aria-hidden="true" />
      <span className="min-w-0 flex-1" title={group.repository?.fullName}>
        <span className="summary-action-label font-medium"><span>{group.changes.length} {group.changes.length === 1 ? "file" : "files"}{group.changes.some(change => change.running) ? "" : " changed"}</span></span>
        {group.label ? <span className="repo-name min-w-0 text-eyebrow font-normal text-muted-foreground"><span>{group.label}</span></span> : null}
      </span>
      {group.lines.kind !== "unknown" ? <RecordedChangeCounts lines={group.lines} /> : null}
      <CaretRightIcon size={14} className="shrink-0 text-muted-foreground" aria-hidden="true" />
    </Button>
  </RecordedChangesDialog>)}</WorkSection>;
}

export function WorkSources({sources, files}: {sources?: FleetSources; files?: readonly {id: string; name: string}[]}) {
  return <WorkSection title="Sources">
    {files?.length ? <AttachedSources files={files} /> : null}
    {sources?.repositories.length ? <RepositorySources sources={sources} /> : files?.length ? null :
      <p className="text-eyebrow leading-eyebrow text-muted-foreground">No sources configured.</p>}
  </WorkSection>;
}

function RepositorySources({sources}: {sources: FleetSources}) {
  const {repositories} = sources;
  const label = repositories.length === 1 ? repositories.join("") : `${repositories.length} repositories`;
  return <details className="group">
    <summary className={cn(SUMMARY_ROW, "min-h-9 cursor-pointer rounded-chip py-sm text-label leading-label focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring max-summary:min-h-11 pointer-coarse:min-h-11")}>
      <GitBranchIcon className={cn(SUMMARY_ICON, "mt-xs self-start")} aria-hidden="true" />
      <span className="min-w-0 flex-1" title={repositories.join(", ")}>
        <strong className="summary-action-label font-medium"><span>{label}</span></strong>
        <small className="block truncate text-eyebrow leading-eyebrow text-text-dim">Policy: {repositoryPolicyLabel(sources)}</small>
      </span>
      <CaretDownIcon className={cn(SUMMARY_ICON, "transition-transform group-open:rotate-180 motion-reduce:transition-none")} aria-hidden="true" />
    </summary>
    <List variant="plain" className="mt-sm space-y-sm pl-6 text-label leading-label">{repositories.map(repository => <ListItem key={repository} className="break-words">
      <a href={`https://github.com/${repository}`} target="_blank" rel="noopener noreferrer" className="hover:underline">{repository}</a>
    </ListItem>)}</List>
  </details>;
}

function repositoryPolicyLabel({access, publication}: FleetSources): string {
  if (access === "read") return "Read only";
  if (publication === "existing_pr") return access === "write" ? "Edit existing PRs" : "Existing PRs";
  if (publication === "event_draft") return access === "write" ? "Open draft PRs" : "Draft PRs";
  return access === "write" ? "Write access" : "Not specified";
}

export function WorkApprovals({approvals, scope}: {approvals?: {count: number; href: string}; scope?: {workspaceId: string; fleetId: string; subject: string; revision: number}}) {
  if (scope && approvals) return <FleetApprovalSummary key={`${scope.subject}:${scope.workspaceId}:${scope.fleetId}`} workspaceId={scope.workspaceId} fleetId={scope.fleetId} revision={scope.revision} count={approvals.count} />;
  if (!approvals?.count) return null;
  return <WorkSection title="Needs your approval" count={approvals.count} attention>
    <Button asChild size="sm" className="min-h-11 summary:min-h-10"><Link href={approvals.href} aria-label={`Open fleet with ${approvals.count} pending ${approvals.count === 1 ? "approval" : "approvals"}`}>Open fleet</Link></Button>
  </WorkSection>;
}
