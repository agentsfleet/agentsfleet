import { CopyButton, IconAction, Time } from "@agentsfleet/design-system";
import { PencilIcon, PencilLineIcon, Trash2Icon } from "lucide-react";
import type { Secret } from "@/lib/api/secrets";
import { SECRET_ROW_DESCRIPTION } from "../copy";

type SecretActionProps = {
  secret: Secret;
  pending: boolean;
  protectedFromDelete: boolean;
  onEdit: (name: string) => void;
  onDelete: (name: string) => void;
};

export function SecretActions({ secret, pending, protectedFromDelete, onEdit, onDelete }: SecretActionProps) {
  const deleteDisabled = pending || protectedFromDelete;
  return (
    <div className="flex justify-end gap-1">
      <IconAction type="button" variant="ghost" onClick={() => onEdit(secret.name)} disabled={pending} label={`Edit secret ${secret.name}`}>
        <PencilIcon size={14} />
      </IconAction>
      <IconAction
        type="button"
        variant="destructive"
        onClick={() => onDelete(secret.name)}
        disabled={deleteDisabled}
        label={protectedFromDelete ? `Secret ${secret.name} is in model setup` : `Delete secret ${secret.name}`}
        title={protectedFromDelete ? "Switch model setup to platform defaults or another secret before deleting this one." : undefined}
      >
        <Trash2Icon size={14} />
      </IconAction>
    </div>
  );
}

export function SecretCreatedCell({ secret }: { secret: Secret }) {
  return <Time value={new Date(secret.created_at)} format="relative" className="font-sans text-body-sm leading-body-sm tabular-nums text-muted-foreground" />;
}

export function SecretNameCell({ secret, pending, onRename }: {
  secret: Secret;
  pending: boolean;
  onRename: (name: string) => void;
}) {
  return (
    <div className="flex min-w-0 items-start gap-1">
      <div className="min-w-0">
        {/* The name is the interpolation key in ${secrets.<name>.<field>}; copying it avoids silent typos. */}
        <div className="flex min-w-0 items-center gap-1">
          <div className="truncate font-mono text-mono leading-mono">{secret.name}</div>
          <CopyButton value={secret.name} label={`Copy secret name: ${secret.name}`} />
        </div>
        <div className="text-label leading-label text-muted-foreground">{SECRET_ROW_DESCRIPTION}</div>
        <div className="sm:hidden text-label leading-label text-muted-foreground">Created <SecretCreatedCell secret={secret} /></div>
      </div>
      <IconAction type="button" variant="ghost" onClick={() => onRename(secret.name)} disabled={pending} label={`Rename secret ${secret.name}`} title="Rename">
        <PencilLineIcon size={14} />
      </IconAction>
    </div>
  );
}
