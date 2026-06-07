"use client";

import Link from "next/link";
import {
  Alert,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@usezombie/design-system";
import type { CredentialSummary } from "@/lib/api/credentials";

export type Step1CredentialProps = {
  workspaceId: string;
  credentials: CredentialSummary[];
  credentialRef: string;
  onCredentialRefChange: (ref: string) => void;
};

/**
 * Step 1 of the self-managed wizard — pick which vault credential holds the
 * provider key. Pure presentation; selection is owned by the parent
 * orchestrator. When the vault is empty it shows a CTA to add one.
 */
export default function Step1Credential({
  workspaceId,
  credentials,
  credentialRef,
  onCredentialRefChange,
}: Step1CredentialProps) {
  const noCredentials = credentials.length === 0;

  return (
    <div className="space-y-2">
      <Label htmlFor="credential-ref">Credential</Label>
      {noCredentials ? (
        <Alert variant="warning" data-testid="provider-key-no-credentials" className="text-xs">
          <span>
            No credentials in this workspace yet.{" "}
            <Link
              href="/credentials"
              className="font-semibold underline"
              data-workspace-id={workspaceId}
            >
              Add a credential first
            </Link>
            {" "}— it must contain JSON fields <code>provider</code>, <code>api_key</code>, and{" "}
            <code>model</code>.
          </span>
        </Alert>
      ) : (
        <Select value={credentialRef} onValueChange={onCredentialRefChange}>
          <SelectTrigger id="credential-ref" aria-label="Credential">
            <SelectValue placeholder="Select a credential" />
          </SelectTrigger>
          <SelectContent>
            {credentials.map((c) => (
              <SelectItem key={c.name} value={c.name}>
                {c.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
    </div>
  );
}
