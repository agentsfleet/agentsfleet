"use client";

import type { ReactElement } from "react";
import type { recordedChanges } from "./recorded-changes";
import { FileTextIcon } from "@phosphor-icons/react/ssr";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@agentsfleet/design-system";

type Props = {changes: ReturnType<typeof recordedChanges>; children: ReactElement};

/** Both entry points inspect the same recorded file facts. */
export function RecordedChangesDialog({changes, children}: Props) {
  const running = changes.some(change => change.running);
  return <Dialog><DialogTrigger asChild>{children}</DialogTrigger>
    <DialogContent className="ph-no-capture ph-no-record">
      <DialogHeader><DialogTitle>Recorded changes</DialogTitle><DialogDescription>{running ? "Recorded writes, including work still in progress." : "Files with confirmed local writes."} Publication and line totals are not available here.</DialogDescription></DialogHeader>
      <ul className="space-y-lg">{changes.map(change => <li key={change.path} className="flex items-start gap-md text-label">
        <FileTextIcon size={16} className="mt-xs shrink-0 text-muted-foreground" aria-hidden="true" /><span className="min-w-0 break-all font-mono">{change.path}{change.running ? <span className="mt-xs block font-sans text-eyebrow text-muted-foreground">Write in progress</span> : null}</span>
      </li>)}</ul>
    </DialogContent>
  </Dialog>;
}
