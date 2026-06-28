"use client";

import { useState } from "react";
import { Button, Refpill, Time } from "@agentsfleet/design-system";
import type { CustomSecretCredential } from "@/lib/api/credentials";
import EditCredentialDialog from "./EditCredentialDialog";

// Custom secrets are named JSON objects a SKILL.md reads by field path. A
// listed entry always holds a value; the reference column shows only the known
// active model credential and never fabricates a usage graph.

type Props = {
  workspaceId: string;
  secrets: CustomSecretCredential[];
  /** The credential name the active model setup references, if it is a custom secret. */
  referencedName?: string | null;
};

const NOT_REFERENCED = "— not referenced yet";
const MODEL_SETUP_REF = "model setup";
const EMPTY_ROW = "No custom secrets stored";

function SecretNameCell({ secret }: { secret: CustomSecretCredential }) {
  return <span className="font-mono text-sm text-foreground">{secret.name}</span>;
}

function SecretRefCell({ referenced }: { referenced: boolean }) {
  if (!referenced) {
    return <span className="text-xs text-text-subtle">{NOT_REFERENCED}</span>;
  }
  // Reusable Refpill primitive (design preview `.refpill`) names what references
  // the secret, instead of bare text.
  return <Refpill>{MODEL_SETUP_REF}</Refpill>;
}

export default function CustomSecretsList({ workspaceId, secrets, referencedName = null }: Props) {
  const [editTarget, setEditTarget] = useState<string | null>(null);

  return (
    <div data-testid="custom-secrets-list">
      <table className="w-full border-collapse text-body-sm">
        <caption className="sr-only">Custom secrets</caption>
        <thead className="bg-surface-deep">
          <tr>
            <th className="px-lg py-md text-left font-mono text-label uppercase tracking-label text-muted-foreground">
              Name
            </th>
            <th className="px-lg py-md text-left font-mono text-label uppercase tracking-label text-muted-foreground">
              Added
            </th>
            <th className="hidden px-lg py-md text-left font-mono text-label uppercase tracking-label text-muted-foreground sm:table-cell">
              Referenced by
            </th>
            <th className="px-lg py-md text-right font-mono text-label uppercase tracking-label text-muted-foreground">
              Action
            </th>
          </tr>
        </thead>
        <tbody>
          {secrets.length === 0 ? (
            <tr className="border-t border-border">
              <td className="px-lg py-lg text-muted-foreground" colSpan={4}>
                {EMPTY_ROW}. Add one below.
              </td>
            </tr>
          ) : (
            secrets.map((secret) => (
              <tr
                key={secret.name}
                className="border-t border-border transition-colors duration-snap ease-snap hover:bg-secondary"
              >
                <td className="px-lg py-md align-middle">
                  <SecretNameCell secret={secret} />
                </td>
                <td className="px-lg py-md align-middle">
                  <Time
                    value={new Date(secret.created_at)}
                    format="relative"
                    tooltip={false}
                    className="text-body-sm text-muted-foreground"
                  />
                </td>
                <td className="hidden px-lg py-md align-middle sm:table-cell">
                  <SecretRefCell referenced={secret.name === referencedName} />
                </td>
                <td className="px-lg py-md text-right align-middle">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setEditTarget(secret.name)}
                    aria-label={`Replace secret ${secret.name}`}
                  >
                    Replace
                  </Button>
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
      <EditCredentialDialog
        workspaceId={workspaceId}
        name={editTarget ?? ""}
        open={editTarget !== null}
        onOpenChange={() => setEditTarget(null)}
      />
    </div>
  );
}
