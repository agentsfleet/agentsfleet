import { TIME_LABEL } from "@/lib/time-labels";
import type { PlatformEntry } from "@/lib/api/platform-library-types";
import { Badge, IconAction, Time, type DataTableColumn } from "@agentsfleet/design-system";
import { EyeIcon, EyeOffIcon, PencilIcon, Trash2Icon } from "lucide-react";
import { preloadEditFleetDialog } from "@/components/domain/island-dynamic/EditFleetDialogDynamic";
import { maySpeculateOnHover } from "@/components/domain/island-dynamic/intent-module-loader";
import { DELETE, EDIT, PUBLISH, UNPUBLISH } from "../library-copy";

type Actions = {busy: boolean; edit: (entry: PlatformEntry) => void; remove: (entry: PlatformEntry) => void; publish: (entry: PlatformEntry, published: boolean) => void};

export function platformColumns(actions: Actions): DataTableColumn<PlatformEntry>[] {
  return [
    {key: "name", primary: true, header: "Name", sortValue: (row) => row.name, cell: (row) => <span className="font-medium">{row.name}</span>},
    {key: "revision", header: "Revision", hideOnMobile: true, cell: (row) => <code className="text-xs">{row.head_revision_id}</code>},
    {key: "status", header: "Status", sortValue: (row) => String(row.published), cell: (row) => <Badge variant={row.published ? "green" : "default"}>{row.published ? "Published" : "Draft"}</Badge>},
    {key: "updated_at", header: TIME_LABEL.updated, hideOnMobile: true, sortValue: (row) => row.updated_at, cell: (row) => <Time value={new Date(row.updated_at)} format="relative" />},
    {key: "actions", header: "Actions", numeric: true, cell: (row) => <RowActions entry={row} actions={actions} />},
  ];
}

function RowActions({entry, actions}: {entry: PlatformEntry; actions: Actions}) {
  return <div className="flex items-center justify-end gap-xs">
    <IconAction variant="ghost" label={EDIT} disabled={actions.busy} onFocus={preloadEditFleetDialog}
      onPointerEnter={() => { if (maySpeculateOnHover()) preloadEditFleetDialog(); }} onClick={() => actions.edit(entry)}><PencilIcon size={14} /></IconAction>
    <IconAction variant="ghost" label={entry.published ? UNPUBLISH : PUBLISH} disabled={actions.busy} onClick={() => actions.publish(entry, !entry.published)}>
      {entry.published ? <EyeOffIcon size={14} /> : <EyeIcon size={14} />}
    </IconAction>
    {!entry.published ? <IconAction variant="destructive" label={DELETE} disabled={actions.busy} onClick={() => actions.remove(entry)}><Trash2Icon size={14} /></IconAction> : null}
  </div>;
}
