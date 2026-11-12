"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger, TooltipButton } from "@agentsfleet/design-system";
import { ArrowUpRightIcon, PlusIcon } from "@phosphor-icons/react/ssr";
import type { Secret } from "@/lib/api/secrets";
import { useCurrentUser } from "@/lib/auth/client";
import { maySpeculateOnHover } from "@/components/domain/island-dynamic/intent-module-loader";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { SECRETS_LOAD, type SecretsLoad } from "./secrets-load";
import { CreateModelForm } from "./CreateModelForm";

type Props = {
  triggerLabel?: string; secondary?: boolean;
  workspaceId: string; secrets: Secret[]; secretsLoad: SecretsLoad;
  onCreated: () => void; onSecretsChanged: () => void; onSecretsNeeded: () => void;
};

export default function AddModelEntryDialog({workspaceId, secrets, secretsLoad, onCreated, onSecretsChanged, onSecretsNeeded, triggerLabel = "Create model", secondary = false}: Props) {
  const [open, setOpen] = useState(false);
  const pending = useRef(false);
  const catalogue = useModelCatalogue();
  const identity = useCurrentUser();
  const ownerReady = identity.isLoaded && identity.isSignedIn && identity.userId !== null;
  const scope = `${workspaceId}:${identity.userId ?? ""}`;
  const previousScope = useRef(scope);
  useEffect(() => {
    if (previousScope.current === scope) return;
    previousScope.current = scope;
    pending.current = false;
    setOpen(false);
  }, [scope]);
  const onPendingChange = useCallback((value: boolean) => { pending.current = value; }, []);
  const complete = () => { pending.current = false; setOpen(false); };

  function changeOpen(next: boolean) {
    if (!next && pending.current) return;
    if (next) { catalogue.preload(); onSecretsNeeded(); }
    setOpen(next);
  }

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <TooltipButton variant={secondary ? "outline" : "default"} type="button" size="sm" disabled={!ownerReady} tooltip="Choose a model and connect its provider" onFocus={catalogue.preload}
          onPointerEnter={() => { if (catalogue.status !== CATALOGUE_STATUS.error && maySpeculateOnHover()) catalogue.preload(); }}>
          {secondary ? null : <PlusIcon size={16} aria-hidden="true" />}{triggerLabel}{secondary ? <ArrowUpRightIcon size={16} aria-hidden="true" /> : null}
        </TooltipButton>
      </DialogTrigger>
      <DialogContent className="max-w-model-dialog" onEscapeKeyDown={event => { if (pending.current) event.preventDefault(); }} onInteractOutside={event => { if (pending.current) event.preventDefault(); }}>
        <DialogHeader>
          <DialogTitle>Create model</DialogTitle>
          <DialogDescription>Choose a model and connect its provider. You can add the key without leaving this form.</DialogDescription>
        </DialogHeader>
        {secretsLoad === SECRETS_LOAD.error ? <Alert variant="destructive">
          <span>Couldn’t load saved keys.</span><Button type="button" variant="link" onClick={onSecretsNeeded}>Try again</Button>
        </Alert> : secretsLoad !== SECRETS_LOAD.ready || !identity.isLoaded ? <output className="text-label text-muted-foreground">Loading model credentials…</output> :
          <CreateModelForm key={`${workspaceId}:${identity.userId}`} workspaceId={workspaceId} secrets={secrets}
            onCreated={onCreated} onSecretsChanged={onSecretsChanged} onCancel={() => changeOpen(false)} onComplete={complete} onPendingChange={onPendingChange} />}
      </DialogContent>
    </Dialog>
  );
}
