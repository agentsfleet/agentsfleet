import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { RecordedToolWrites } from "./recorded-tool-writes";

export type ChangeTotals = {kind: "unknown"} | {kind: "complete" | "partial"; added: number; removed: number};
export type ChangeRepository = {id: string; label: string; fullName: string};
/** Explicit repository-index evidence for one event and path, never a directory-name guess. */
export type RecordedChangeRepository = {
  eventId: string; path: string; repository: ChangeRepository;
  /** A complete per-file receipt for this event, independent of clipped tool arguments. */
  lines?: {added: number; removed: number};
};
export type RecordedChange = {path: string; running: boolean; lines: ChangeTotals; repository?: ChangeRepository};
type Options = {includeRunning?: boolean; repositories?: readonly RecordedChangeRepository[]};
type ChangeGroup = {repository: ChangeRepository | undefined; label: string; changes: RecordedChange[]; lines: ChangeTotals};

const UNKNOWN: ChangeTotals = {kind: "unknown"};
const EMPTY_REPOSITORIES: readonly RecordedChangeRepository[] = [];
const bindingKey = (event: string, path: string) => JSON.stringify([event, path]);
const recordings = new RecordedToolWrites();

/** Counts describe recorded operations, rather than a net diff against a repository revision. */
export function recordedChanges(events: readonly FleetEvent[], {includeRunning = true, repositories = EMPTY_REPOSITORIES}: Options = {}): RecordedChange[] {
  const bindings = new Map(repositories.map(binding => [bindingKey(binding.eventId, binding.path), binding]));
  const paths = new Map<string, RecordedChange>();
  for (const event of events) for (const change of changesInEvent(event, includeRunning, bindings)) {
    const key = JSON.stringify([change.repository?.id, change.path]);
    const previous = paths.get(key);
    paths.delete(key);
    paths.set(key, {...change, lines: previous ? mergeTotals(previous.lines, change.lines) : change.lines});
  }
  return [...paths.values()].reverse();
}

export function recordedChangeTotals(changes: readonly RecordedChange[]): ChangeTotals {
  const [first, ...rest] = changes;
  return first ? rest.map(change => change.lines).reduce(mergeTotals, first.lines) : {kind: "complete", added: 0, removed: 0};
}

/** Repository identity comes only from explicit bindings; equal short names stay distinguishable. */
export function groupRecordedChanges(changes: readonly RecordedChange[]): ChangeGroup[] {
  const groups = new Map<string | undefined, ChangeGroup>();
  for (const change of changes.toReversed()) {
    const id = change.repository?.id;
    const group: ChangeGroup = groups.get(id) ?? {repository: change.repository, label: change.repository?.label ?? "Recorded files", changes: [], lines: UNKNOWN};
    group.changes.unshift(change);
    groups.set(id, group);
  }
  const rows = [...groups.values()];
  const labels = new Map<string, number>();
  for (const group of rows) if (group.repository) labels.set(group.repository.label, (labels.get(group.repository.label) ?? 0) + 1);
  for (const group of rows) {
    if (group.repository && labels.get(group.repository.label) !== 1) group.label = group.repository.fullName;
    group.lines = recordedChangeTotals(group.changes);
  }
  return rows;
}

function changesInEvent(event: FleetEvent, includeRunning: boolean, bindings: ReadonlyMap<string, RecordedChangeRepository>): RecordedChange[] {
  const paths = new Map<string, RecordedChange>();
  for (const tool of event.tools ?? []) for (const write of recordings.read(tool)) {
    const {path, running} = write;
    if (running && !includeRunning) continue;
    const receipt = bindings.get(bindingKey(event.id, path))?.lines;
    const totals = running || receipt ? undefined : write.totals();
    const lines: ChangeTotals = totals ? {kind: "complete", ...totals} : UNKNOWN;
    const previous = paths.get(path);
    paths.delete(path);
    paths.set(path, {path, running, lines: previous ? mergeTotals(previous.lines, lines) : lines});
  }
  return [...paths.values()].map<RecordedChange>(change => {
    const binding = bindings.get(bindingKey(event.id, change.path));
    if (!binding) return change;
    return {...change, repository: binding.repository,
      lines: !change.running && binding.lines ? {kind: "complete", ...binding.lines} : change.lines};
  });
}

function mergeTotals(left: ChangeTotals, right: ChangeTotals): ChangeTotals {
  if (left.kind === "unknown") return right.kind === "unknown" ? UNKNOWN : {...right, kind: "partial"};
  if (right.kind === "unknown") return {...left, kind: "partial"};
  return {kind: left.kind === "complete" && right.kind === "complete" ? "complete" : "partial",
    added: left.added + right.added, removed: left.removed + right.removed};
}
