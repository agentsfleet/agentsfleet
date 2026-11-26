"use client";

import { useMemo } from "react";
import { CaretUpIcon, GitDiffIcon } from "@phosphor-icons/react/ssr";
import { Button } from "@agentsfleet/design-system";
import { useFleetDurableTaskView } from "../../tasks/FleetTaskProvider";
import type { RecordedChange } from "./recorded-changes";
import { groupRecordedChanges, recordedChanges, recordedChangeTotals } from "./recorded-changes";
import { RecordedChangeCounts, RecordedChangesDialog } from "./RecordedChangesDialog";

/** Only confirmed writes from this fleet's recorded turns enter the count. */
export function ChangesStrip() {
  const view = useFleetDurableTaskView();
  const subject = view?.subject;
  const turns = view?.turns;
  const repositories = view?.changeRepositories;
  const changes = useMemo(() => subject && turns ? recordedChanges(turns, {includeRunning: false, repositories}) : [], [subject, turns, repositories]);
  return <ChangedFilesFlap changes={changes} />;
}

/** The same presentation accepts a separately recorded repository-index receipt. */
export function ChangedFilesFlap({changes}: {changes: readonly RecordedChange[]}) {
  if (!changes.length) return null;
  const lines = recordedChangeTotals(changes);
  return <div data-changes-strip="" className="flex justify-center pb-md"><RecordedChangesDialog groups={groupRecordedChanges(changes)}>
    <Button variant="secondary" className="h-auto min-h-9 w-fit max-w-full flex-wrap gap-sm rounded-md border border-border bg-muted px-lg py-sm text-eyebrow text-muted-foreground pointer-coarse:min-h-11">
      <GitDiffIcon aria-hidden="true" />
      <span className="font-medium">{changes.length} {changes.length === 1 ? "file changed" : "files changed"}</span>
      {lines.kind !== "unknown" ? <RecordedChangeCounts lines={lines} /> : null}
      <CaretUpIcon aria-hidden="true" />
    </Button>
  </RecordedChangesDialog></div>;
}
