"use client";

import { BoxIcon, LockKeyholeIcon } from "lucide-react";
import { Badge, Label } from "@agentsfleet/design-system";

export function PolicyEngineField() {
  return <div className="space-y-sm">
    <Label asChild><span>Sandbox engine</span></Label>
    <div className="flex items-center justify-between gap-md rounded-lg border border-border bg-muted p-md">
      <div className="flex items-center gap-md">
        <BoxIcon size={18} className="text-pulse" aria-hidden="true" />
        <div><p className="text-body-sm font-semibold">Bubblewrap</p>
          <p className="text-label text-muted-foreground">Linux sandbox runtime</p></div>
      </div>
      <Badge variant="default"><LockKeyholeIcon size={12} aria-hidden="true" /> Default engine</Badge>
    </div>
  </div>;
}
