"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@agentsfleet/design-system";
import { PlusIcon } from "@phosphor-icons/react/ssr";
import type { Secret } from "@/lib/api/secrets";
import type { SecretsLoad } from "./secrets-load";
import { useCurrentUser } from "@/lib/auth/client";
import { maySpeculateOnHover } from "@/components/domain/shared/dialogs/intent-module-loader";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { CreateModelForm } from "./CreateModelForm";

type Props = {
  workspaceId: string; secrets: Secret[]; secretsLoad: SecretsLoad;
  onCreated: () => void; onSecretsChanged: () => void; onSecretsNeeded: () => void;
};

export function AddModelEntryDialog({workspaceId, secrets, secretsLoad, onCreated, onSecretsChanged, onSecretsNeeded}: Props) {
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
    if (next) { catalogue.preloadAdvanced(); onSecretsNeeded(); }
    setOpen(next);
  }

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <Button type="button" size="sm" disabled={!ownerReady} onFocus={catalogue.preload}
          onPointerEnter={() => { if (catalogue.status !== CATALOGUE_STATUS.error && maySpeculateOnHover()) catalogue.preload(); }}>
          <PlusIcon aria-hidden="true" />Create model
        </Button>
      </DialogTrigger>
      <DialogContent size="wide" className="[&_[data-dialog-close]]:text-foreground max-summary:[&_[data-dialog-close]]:right-lg" onEscapeKeyDown={event => { if (pending.current) event.preventDefault(); }} onInteractOutside={event => { if (pending.current) event.preventDefault(); }}>
        <DialogHeader className="gap-lg pb-3xl max-summary:pb-7">
          <DialogTitle>Create model</DialogTitle>
          <DialogDescription>Choose a model and connect its provider. You can add the key without leaving this form.</DialogDescription>
        </DialogHeader>
        {ownerReady ? <CreateModelForm key={scope} workspaceId={workspaceId} secrets={secrets}
            secretsLoad={secretsLoad} onSecretsNeeded={onSecretsNeeded}
            onCreated={onCreated} onSecretsChanged={onSecretsChanged} onCancel={() => changeOpen(false)} onComplete={complete} onPendingChange={onPendingChange} /> : null}
      </DialogContent>
    </Dialog>
  );
}
