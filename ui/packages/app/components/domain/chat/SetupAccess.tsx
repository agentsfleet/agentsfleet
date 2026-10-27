"use client";

import type { Definition } from "@/lib/api/draft-types";
import { useState } from "react";
import nextDynamic from "next/dynamic";
import { z } from "zod";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@agentsfleet/design-system";
import { readConnectorCatalogAction, startConnectAction } from "@/app/(dashboard)/w/[workspaceId]/integrations/connector-actions";

const AddSecretForm = nextDynamic(() => import("@/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretForm"), {
  loading: () => <output>Loading access form…</output>,
});

const CatalogSchema = z.array(z.object({id: z.string(), display_name: z.string(), configured: z.boolean()}));
const NetworkSchema = z.object({"x-agentsfleet": z.object({network: z.object({allow: z.array(z.string())})})});
const SECRET_REFERENCE = /\$\{secrets\.([A-Za-z0-9_-]+)\.([A-Za-z0-9_]+)\}/g;
const FIELD_CAPTURE_INDEX = 2;
const CONNECT = "Connect";

export function SetupAccess({workspace, name, definition, onRefresh}: {workspace: string; name: string; definition?: Definition; onRefresh?: () => Promise<void>}) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [authorizationStarted, setAuthorizationStarted] = useState(false);
  const [savedName, setSavedName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const label = name.charAt(0).toUpperCase() + name.slice(1);
  const fields = [...new Set([...((definition?.instructions_md ?? "").matchAll(SECRET_REFERENCE))]
    .filter(match => match[1] === name).flatMap(match => match.slice(FIELD_CAPTURE_INDEX)))];
  const network = NetworkSchema.safeParse(definition?.config);
  const allowedHosts = network.success ? network.data["x-agentsfleet"].network.allow : [];
  const initialHost = fields.length ? allowedHosts.length === 1 ? allowedHosts[0] : "" : undefined;
  const connect = async (): Promise<void> => {
    setBusy(true); setError(null);
    let popup: Window | null = null;
    try {
      const catalog = await readConnectorCatalogAction(workspace);
      if (!catalog.ok) {setError(catalog.error); return;}
      const parsed = CatalogSchema.safeParse(catalog.data);
      if (!parsed.success) {setError("Access options could not be loaded. Try again."); return;}
      const provider = parsed.data.find(entry => entry.id === name);
      if (!provider) {setOpen(true); return;}
      if (!provider.configured) {setError(`${provider.display_name} needs to be enabled by a workspace administrator.`); return;}
      popup = window.open("about:blank", "_blank");
      if (!popup) {setError("Allow a new browser tab, then connect again."); return;}
      popup.opener = null;
      const result = await startConnectAction(provider.id, workspace);
      if (!result.ok) {popup.close(); setError(result.error); return;}
      const destination = new URL(result.data.install_url);
      if (destination.protocol !== "https:") {popup.close(); setError("The provider returned an invalid authorization URL."); return;}
      popup.location.replace(destination.href);
      setAuthorizationStarted(true);
    } catch {popup?.close(); setError("Access could not be opened. Try again.");}
    finally {setBusy(false);}
  };
  const refresh = async (): Promise<void> => {
    setBusy(true); setError(null);
    try {await onRefresh?.();}
    catch {setError("Readiness could not be checked. Try checking again.");}
    finally {setBusy(false);}
  };
  return <div className="space-y-xs">
    <div className="flex flex-wrap items-center gap-sm">
      <Button variant="link" size="sm" disabled={busy} onClick={() => {void connect();}}>{busy ? "Checking access…" : `${CONNECT} ${label}`}</Button>
      {(authorizationStarted || savedName) && onRefresh ? <Button variant="ghost" size="sm" disabled={busy} onClick={() => {void refresh();}}>Check again</Button> : null}
    </div>
    {authorizationStarted ? <p className="text-label text-muted-foreground">Finish authorizing in the new tab, then check again here.</p> : null}
    {savedName ? <p className="text-label text-muted-foreground">Access saved as {savedName}.</p> : null}
    {error ? <Alert variant="destructive">{error}</Alert> : null}
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="ph-no-capture ph-no-record"><DialogHeader><DialogTitle>{CONNECT} {label}</DialogTitle>
        <DialogDescription>Add this access securely. Its value stays out of setup chat.</DialogDescription></DialogHeader>
        <AddSecretForm workspaceId={workspace} initialName={name} initialFields={fields.filter(field => field !== "host")} initialHost={initialHost}
          onCancel={() => setOpen(false)} onDone={saved => {setOpen(false); setSavedName(saved); void refresh();}} />
      </DialogContent>
    </Dialog>
  </div>;
}
