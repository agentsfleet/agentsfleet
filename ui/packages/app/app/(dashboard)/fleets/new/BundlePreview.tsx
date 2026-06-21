"use client";

import { useState } from "react";
import Link from "next/link";
import { Alert, Badge, Button, Input, Spinner } from "@agentsfleet/design-system";
import { missingCredentials, WORKSPACE_CREDENTIALS_PATH } from "@/lib/fleet-credentials";

const CONNECTED_LABEL = "connected";
const MISSING_LABEL = "missing";

type Props = {
  name: string;
  credentials: string[];
  tools: string[];
  networkHosts: string[];
  // The workspace's present credential names, or `null` when the vault could
  // not be read at page load — in which case the preview cannot tell present
  // from missing, so it shows neither and leaves create ungated (the server's
  // 424 stays authoritative). An empty array means a readable, empty vault.
  presentCredentialNames: string[] | null;
  // The bundle's own SKILL.md name, shown as the placeholder default when known
  // (GitHub snapshots); undefined for templates, whose real name resolves
  // server-side at import.
  defaultFleetName?: string;
  creating: boolean;
  createError: string | null;
  onCreate: (nameOverride?: string) => void;
  onBack: () => void;
};

// Install preview: shows the credentials, tools, and network
// a bundle needs before creating the Fleet. Required credentials are workspace
// service credentials — missing ones link to the workspace credentials flow, and
// the copy keeps them distinct from the tenant model provider. Create is gated
// until every required credential is present in a readable vault. An optional
// name lets the same bundle back several fleets in one workspace.
export function BundlePreview({
  name,
  credentials,
  tools,
  networkHosts,
  presentCredentialNames,
  defaultFleetName,
  creating,
  createError,
  onCreate,
  onBack,
}: Props) {
  const [fleetName, setFleetName] = useState("");
  const missing =
    presentCredentialNames === null
      ? null
      : new Set(missingCredentials(credentials, presentCredentialNames));
  const ready = missing === null || missing.size === 0;

  return (
    <div className="max-w-2xl space-y-5">
      <div className="space-y-1">
        <h2 className="font-mono text-heading text-foreground">Review what it needs</h2>
        <p className="text-sm text-muted-foreground">{name}</p>
      </div>

      <div className="space-y-1.5">
        <label
          htmlFor="fleet-name"
          className="font-mono text-xs uppercase tracking-label text-muted-foreground"
        >
          Name
        </label>
        <Input
          id="fleet-name"
          value={fleetName}
          onChange={(event) => setFleetName(event.target.value)}
          placeholder={
            defaultFleetName ? `Defaults to "${defaultFleetName}"` : "Leave blank to use the bundle's name"
          }
          autoComplete="off"
          spellCheck={false}
          disabled={creating}
          className="max-w-sm font-mono text-sm"
        />
        <p className="text-xs text-muted-foreground">
          Lowercase letters, digits, and hyphens. Name it to run several of the
          same template in one workspace.
        </p>
      </div>

      <div className="space-y-2">
        <p className="font-mono text-xs uppercase tracking-label text-muted-foreground">
          Workspace credentials
        </p>
        <p className="text-xs text-muted-foreground">
          Service credentials live in your workspace vault — separate from the
          tenant model provider you set in Settings → Models.
        </p>
        {credentials.length > 0 ? (
          <ul className="space-y-1.5">
            {credentials.map((cred) => (
              <li key={cred} className="flex items-center gap-3 text-sm">
                <span className="font-mono text-foreground">{cred}</span>
                {missing === null ? null : missing.has(cred) ? (
                  <>
                    <Badge variant="amber">{MISSING_LABEL}</Badge>
                    <Button asChild variant="ghost" size="sm">
                      <Link href={WORKSPACE_CREDENTIALS_PATH}>Connect</Link>
                    </Button>
                  </>
                ) : (
                  <Badge variant="green">{CONNECTED_LABEL}</Badge>
                )}
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-muted-foreground">No credentials required.</p>
        )}
      </div>

      {tools.length > 0 ? <PreviewFacts label="Tools" values={tools} /> : null}
      {networkHosts.length > 0 ? <PreviewFacts label="Network" values={networkHosts} /> : null}

      {createError ? <Alert variant="destructive">{createError}</Alert> : null}
      {!ready ? (
        <p className="text-sm text-warning">Connect the required credentials, then create.</p>
      ) : null}

      <div className="flex gap-2 pt-2">
        <Button
          type="button"
          onClick={() => onCreate(fleetName.trim() || undefined)}
          disabled={!ready || creating}
          aria-busy={creating}
          size="sm"
        >
          {creating ? <Spinner size="sm" label="Creating…" /> : "Create teammate"}
        </Button>
        <Button type="button" onClick={onBack} disabled={creating} variant="ghost" size="sm">
          Back
        </Button>
      </div>
    </div>
  );
}

function PreviewFacts({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="space-y-1.5">
      <p className="font-mono text-xs uppercase tracking-label text-muted-foreground">{label}</p>
      <div className="flex flex-wrap gap-1.5">
        {values.map((value) => (
          <Badge key={value} variant="default">
            {value}
          </Badge>
        ))}
      </div>
    </div>
  );
}
