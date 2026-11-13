"use client";

import { CaretUpIcon, GitDiffIcon } from "@phosphor-icons/react/ssr";
import { Button } from "@agentsfleet/design-system";
import { useFleetTaskView } from "./FleetTaskProvider";
import { recordedChanges } from "./recorded-changes";
import { RecordedChangesDialog } from "./RecordedChangesDialog";

/** Only confirmed writes from this fleet's recorded events enter the count. */
export function ChangesStrip() {
  const view = useFleetTaskView();
  const changes = view?.subject ? recordedChanges(view.events ?? [], {includeRunning: false}) : [];
  if (!changes.length) return null;
  return <div data-changes-strip="" className="flex justify-center pb-md"><RecordedChangesDialog changes={changes}>
    <Button variant="secondary" className="h-9.5 min-h-9.5 pointer-coarse:min-h-9.5 w-fit gap-md rounded-md bg-muted px-lg py-md text-eyebrow text-muted-foreground [&_svg]:size-[15px]">
      <GitDiffIcon size={16} aria-hidden="true" />
      <span className="font-medium">{changes.length} {changes.length === 1 ? "file changed" : "files changed"}</span>
      <CaretUpIcon size={16} aria-hidden="true" />
    </Button>
  </RecordedChangesDialog></div>;
}
