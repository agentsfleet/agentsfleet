"use client";

import { useState } from "react";
import { Badge, Button, DataTable, EmptyState, type DataTableColumn } from "@agentsfleet/design-system";
import { KeyRoundIcon } from "lucide-react";
import type { CredentialSummary } from "@/lib/api/credentials";
import EditCredentialDialog from "./EditCredentialDialog";

// Custom secrets are arbitrary NAME=value vault entries a SKILL.md reads by
// name. A listed entry always holds a value, so its status reads "Set"; the
// "Referenced by" column shows only the KNOWN reference — the active model
// credential — and never fabricates a usage graph (Dimension 4.3 / Invariant:
// referenced-by is best-effort, not a synthesized dependency map).

type Props = {
  workspaceId: string;
  secrets: CredentialSummary[];
  /** The credential name the active model setup references, if it is a custom secret. */
  referencedName?: string | null;
};

const SET_STATUS = "Set";
const NOT_REFERENCED = "— not referenced yet";
const MODEL_SETUP_REF = "model setup";

function SecretNameCell({ secret }: { secret: CredentialSummary }) {
  return <span className="font-mono text-sm text-foreground">{secret.name}</span>;
}

function SecretStatusCell() {
  return (
    <Badge variant="green" className="normal-case tracking-normal">
      {SET_STATUS}
    </Badge>
  );
}

function SecretRefCell({ referenced }: { referenced: boolean }) {
  if (!referenced) {
    return <span className="text-xs text-text-subtle">{NOT_REFERENCED}</span>;
  }
  return <span className="font-mono text-xs text-muted-foreground">{MODEL_SETUP_REF}</span>;
}

function buildColumns(
  referencedName: string | null,
  onReplace: (name: string) => void,
): DataTableColumn<CredentialSummary>[] {
  return [
    { key: "name", header: "Name", cell: (s) => <SecretNameCell secret={s} /> },
    { key: "status", header: "Status", cell: () => <SecretStatusCell /> },
    {
      key: "refs",
      header: "Referenced by",
      hideOnMobile: true,
      cell: (s) => <SecretRefCell referenced={s.name === referencedName} />,
    },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (s) => (
        <div className="flex justify-end">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onReplace(s.name)}
            aria-label={`Replace secret ${s.name}`}
          >
            Replace
          </Button>
        </div>
      ),
    },
  ];
}

export default function CustomSecretsList({ workspaceId, secrets, referencedName = null }: Props) {
  const [editTarget, setEditTarget] = useState<string | null>(null);

  if (secrets.length === 0) {
    return (
      <EmptyState
        icon={<KeyRoundIcon size={28} />}
        title="No custom secrets yet"
        description="Add a NAME=value secret your fleets read by name."
      />
    );
  }

  return (
    <div className="space-y-3" data-testid="custom-secrets-list">
      <DataTable
        columns={buildColumns(referencedName, setEditTarget)}
        rows={secrets}
        rowKey={(s) => s.name}
        caption="Custom secrets"
      />
      <EditCredentialDialog
        workspaceId={workspaceId}
        name={editTarget ?? ""}
        open={editTarget !== null}
        onOpenChange={() => setEditTarget(null)}
      />
    </div>
  );
}
