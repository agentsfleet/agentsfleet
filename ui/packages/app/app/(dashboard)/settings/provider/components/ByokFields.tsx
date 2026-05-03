"use client";

import Link from "next/link";
import {
  Alert,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@usezombie/design-system";
import type { CredentialSummary } from "@/lib/api/credentials";

export type ByokFieldsProps = {
  workspaceId: string;
  credentials: CredentialSummary[];
  credentialRef: string;
  onCredentialRefChange: (ref: string) => void;
  modelOverride: string;
  onModelOverrideChange: (value: string) => void;
};

/**
 * BYOK-specific form fields: credential dropdown + model override. Pure
 * presentation. Renders an "add a credential first" CTA when the workspace
 * vault is empty so the parent doesn't need to branch on emptiness for the
 * primary form path.
 */
export default function ByokFields({
  workspaceId,
  credentials,
  credentialRef,
  onCredentialRefChange,
  modelOverride,
  onModelOverrideChange,
}: ByokFieldsProps) {
  const noCredentials = credentials.length === 0;

  return (
    <div className="space-y-4 border-l-2 border-border pl-4 animate-in fade-in-0 slide-in-from-top-2 duration-300 ease-out">
      <div className="space-y-2">
        <Label htmlFor="credential-ref">Credential</Label>
        {noCredentials ? (
          <Alert
            variant="warning"
            data-testid="byok-no-credentials"
            className="text-xs"
          >
            <span>
              No credentials in this workspace yet.{" "}
              <Link
                href="/credentials"
                className="font-semibold underline"
                data-workspace-id={workspaceId}
              >
                Add a credential first
              </Link>
              {" "}— it must contain JSON fields <code>provider</code>,{" "}
              <code>api_key</code>, and <code>model</code>.
            </span>
          </Alert>
        ) : (
          <>
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
          </>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor="model-override">Model override (optional)</Label>
        <Input
          id="model-override"
          name="model"
          value={modelOverride}
          onChange={(e) => onModelOverrideChange(e.target.value)}
          placeholder="leave blank to use the credential's model field"
          spellCheck={false}
        />
        <p className="text-xs text-muted-foreground">
          Must appear in the public model-caps catalogue. Leave blank to use the model stored alongside the API key in the vault.
        </p>
      </div>
    </div>
  );
}
