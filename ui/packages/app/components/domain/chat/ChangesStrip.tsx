"use client";

import { CaretUpIcon, FileTextIcon, GitDiffIcon } from "@phosphor-icons/react/ssr";
import { Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@agentsfleet/design-system";
import { useFleetTaskView } from "./FleetTaskProvider";
import { recordedChanges } from "./WorkContextSections";

/** Only confirmed writes from this fleet's recorded events enter the count. */
export function ChangesStrip() {
  const view = useFleetTaskView();
  const changes = view?.subject ? recordedChanges(view.events ?? [], {includeRunning: false}) : [];
  if (!changes.length) return null;
  return <div data-changes-strip="" className="flex justify-center pb-md"><Dialog>
    <DialogTrigger asChild><Button variant="secondary" className="h-9.5 min-h-9.5 pointer-coarse:min-h-9.5 w-fit gap-md rounded-md bg-muted px-lg py-md text-eyebrow text-muted-foreground [&_svg]:size-[15px]">
      <GitDiffIcon size={16} aria-hidden="true" />
      <span className="font-medium">{changes.length} {changes.length === 1 ? "file changed" : "files changed"}</span>
      <CaretUpIcon size={16} aria-hidden="true" />
    </Button></DialogTrigger>
    <DialogContent className="ph-no-capture ph-no-record">
      <DialogHeader><DialogTitle>Recorded changes</DialogTitle><DialogDescription>Files with confirmed local writes. Publication and line totals are not available here.</DialogDescription></DialogHeader>
      <ul className="space-y-lg">{changes.map(change => <li key={change.path} className="flex items-start gap-md text-label">
        <FileTextIcon size={16} className="mt-xs shrink-0 text-muted-foreground" aria-hidden="true" /><span className="break-all font-mono">{change.path}</span>
      </li>)}</ul>
    </DialogContent>
  </Dialog></div>;
}
