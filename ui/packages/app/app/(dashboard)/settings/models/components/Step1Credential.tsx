"use client";

import { useState } from "react";
import Link from "next/link";
import {
  Button,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@usezombie/design-system";
import type { CredentialSummary } from "@/lib/api/credentials";
import type { ModelCap } from "@/lib/api/model_caps";
import InlineProviderKeyCreate from "./InlineProviderKeyCreate";

export type Step1CredentialProps = {
  workspaceId: string;
  credentials: CredentialSummary[];
  catalogue: ModelCap[];
  credentialRef: string;
  onCredentialRefChange: (ref: string) => void;
};

/**
 * Step 1 of the self-managed wizard — pick or create the vault credential that
 * holds the provider key. An empty vault is no longer a dead-end: the inline
 * create form shows directly. Selection/creation is owned by the parent
 * orchestrator (a freshly created key is selected via onCredentialRefChange).
 */
export default function Step1Credential({
  workspaceId,
  credentials,
  catalogue,
  credentialRef,
  onCredentialRefChange,
}: Step1CredentialProps) {
  const hasCredentials = credentials.length > 0;
  const [creating, setCreating] = useState(false);

  // Empty vault → show the create form straight away rather than a dead-end.
  const showCreate = creating || !hasCredentials;

  function onCreated(name: string) {
    onCredentialRefChange(name);
    setCreating(false);
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <Label htmlFor="credential-ref">Credential</Label>
        {hasCredentials ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => setCreating((v) => !v)}
            aria-expanded={creating}
          >
            {creating ? "Cancel" : "+ New key"}
          </Button>
        ) : null}
      </div>

      {hasCredentials ? (
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
      ) : null}

      {showCreate ? (
        <InlineProviderKeyCreate workspaceId={workspaceId} catalogue={catalogue} onCreated={onCreated} />
      ) : null}

      <p className="text-xs text-muted-foreground">
        <Link href="/credentials" className="underline" data-workspace-id={workspaceId}>
          Manage all credentials →
        </Link>
      </p>
    </div>
  );
}
