"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger, Skeleton, Spinner } from "@agentsfleet/design-system";
import { ArrowUpRightIcon, PlusIcon } from "@phosphor-icons/react/ssr";
import type { Secret } from "@/lib/api/secrets";
import { useCurrentUser } from "@/lib/auth/client";
import { maySpeculateOnHover } from "@/components/domain/shared/dialogs/intent-module-loader";
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
    if (next) { catalogue.preloadAdvanced(); onSecretsNeeded(); }
    setOpen(next);
  }

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <Button variant={secondary ? "outline" : "default"} type="button" size="sm" disabled={!ownerReady} onFocus={catalogue.preload}
          onPointerEnter={() => { if (catalogue.status !== CATALOGUE_STATUS.error && maySpeculateOnHover()) catalogue.preload(); }}>
          {secondary ? null : <PlusIcon aria-hidden="true" />}{triggerLabel}{secondary ? <ArrowUpRightIcon aria-hidden="true" /> : null}
        </Button>
      </DialogTrigger>
      <DialogContent size="wide" className="[&_[data-dialog-close]]:text-foreground max-summary:[&_[data-dialog-close]]:right-lg" onEscapeKeyDown={event => { if (pending.current) event.preventDefault(); }} onInteractOutside={event => { if (pending.current) event.preventDefault(); }}>
        <DialogHeader className="gap-lg pb-3xl max-summary:pb-7">
          <DialogTitle>Create model</DialogTitle>
          <DialogDescription>Choose a model and connect its provider. You can add the key without leaving this form.</DialogDescription>
        </DialogHeader>
        {secretsLoad === SECRETS_LOAD.error ? <Alert variant="destructive">
          <span>Couldn’t load saved keys.</span><Button type="button" variant="link" onClick={onSecretsNeeded}>Try again</Button>
        </Alert> : secretsLoad !== SECRETS_LOAD.ready || !identity.isLoaded ? <ModelFormLoading /> :
          <CreateModelForm key={`${workspaceId}:${identity.userId}`} workspaceId={workspaceId} secrets={secrets}
            onCreated={onCreated} onSecretsChanged={onSecretsChanged} onCancel={() => changeOpen(false)} onComplete={complete} onPendingChange={onPendingChange} />}
      </DialogContent>
    </Dialog>
  );
}

function ModelFormLoading() {
  return <div className="space-y-xl">
    <div aria-hidden="true" className="grid gap-xl summary:grid-cols-2">
      {["Provider", "Model"].map(label => <div key={label} className="space-y-md">
        <span className="block text-label font-medium">{label}</span>
        <Skeleton className="h-11 rounded-control" />
      </div>)}
    </div>
    <Spinner size="sm" label="Checking saved keys…" />
  </div>;
}
