"use client";

import { CaretUpIcon, GitDiffIcon } from "@phosphor-icons/react/ssr";
import { Button } from "@agentsfleet/design-system";
import { useFleetTaskView } from "../../tasks/FleetTaskProvider";
import type { RecordedChange } from "./recorded-changes";
import { recordedChanges, recordedChangeTotals } from "./recorded-changes";
import { RecordedChangeCounts, RecordedChangesDialog } from "./RecordedChangesDialog";

/** Only confirmed writes from this fleet's recorded events enter the count. */
export function ChangesStrip() {
  const view = useFleetTaskView();
  const changes = view?.subject ? recordedChanges(view.events ?? [], {includeRunning: false, repositories: view.changeRepositories}) : [];
  return <ChangedFilesFlap changes={changes} />;
}

/** The same presentation accepts a separately recorded repository-index receipt. */
export function ChangedFilesFlap({changes}: {changes: readonly RecordedChange[]}) {
  if (!changes.length) return null;
  const lines = recordedChangeTotals(changes);
  return <div data-changes-strip="" className="flex justify-center pb-md"><RecordedChangesDialog changes={changes}>
    <Button variant="secondary" className="h-auto min-h-9 pointer-coarse:min-h-11 w-fit max-w-full flex-wrap gap-sm rounded-md border border-border bg-muted px-lg py-sm text-eyebrow text-muted-foreground [&_svg]:size-4">
      <GitDiffIcon size={16} aria-hidden="true" />
      <span className="font-medium">{changes.length} {changes.length === 1 ? "file changed" : "files changed"}</span>
      {lines.kind !== "unknown" ? <RecordedChangeCounts lines={lines} /> : null}
      <CaretUpIcon size={16} aria-hidden="true" />
    </Button>
  </RecordedChangesDialog></div>;
}
