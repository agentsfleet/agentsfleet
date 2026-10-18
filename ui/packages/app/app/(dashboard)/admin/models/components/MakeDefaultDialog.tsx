"use client";

import { useId, useState, useTransition } from "react";
import {
  Alert,
  AlertDescription,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Spinner,
} from "@agentsfleet/design-system";
import { type AdminModel } from "@/lib/api/admin_model_library";
import { ProviderBadge } from "@/components/domain/ModelCells";
import { OPENAI_COMPATIBLE_PROVIDER } from "@/lib/types";
import { presentErrorString } from "@/lib/errors";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { setPlatformDefaultAction } from "../actions";

const LABEL = "block text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-1.5";

// The row owns provider configuration; this dialog collects only its credential.
export default function MakeDefaultDialog({
  model,
  onOpenChange,
  onDone,
}: {
  model: AdminModel;
  onOpenChange: (open: boolean) => void;
  onDone: () => void;
}) {
  const isCustom = model.provider === OPENAI_COMPATIBLE_PROVIDER;
  const keyId = useId();
  const [apiKey, setApiKey] = useState("");
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const trimmedApiKey = apiKey.trim();
  const canSave = trimmedApiKey !== "" && !!model.inference;

  function save() {
    setApiError(null);
    startTransition(async () => {
      const r = await setPlatformDefaultAction({
        provider: model.provider,
        model: model.model_id,
        api_key: trimmedApiKey,
        base_url: isCustom ? model.inference?.base_url : undefined,
      });
      if (!r.ok) {
        setApiError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "set the platform default" }));
        return;
      }
      captureProductEvent(EVENTS.platform_default_set, { provider: model.provider, model: model.model_id, is_custom: isCustom });
      onDone();
      onOpenChange(false);
    });
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Make this the platform default</DialogTitle>
          {/* A div, not the paragraph Radix renders by default: the provider
              badge is a block element, and a div inside a p is invalid. */}
          <DialogDescription asChild>
            <div>
            Every fleet runs <ProviderBadge provider={model.provider} />{" "}
            <span className="font-mono">{model.model_id}</span> when a user hasn&apos;t brought their own key,
            billed at its catalogue rate. Enter the provider API key — it&apos;s stored in your vault and
            never shown again.
            </div>
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {!model.inference ? <Alert><AlertDescription>Edit this model to configure its inference API, URL and output limit before making it the default.</AlertDescription></Alert> : null}
          <div>
            <label className={LABEL} htmlFor={keyId}>API key</label>
            <Input
              id={keyId}
              type="password"
              autoComplete="off"
              placeholder="stored in your workspace vault; never shown again"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
            />
          </div>

          {model.inference ? <p className="break-all text-sm text-muted-foreground">Inference URL: {model.inference.base_url}</p> : null}

          {apiError ? <p className="text-sm text-destructive">{apiError}</p> : null}
        </div>

        <DialogFooter className="flex-col gap-2 sm:flex-row sm:gap-2">
          <Button type="button" variant="ghost" disabled={pending} onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button type="button" disabled={!canSave || pending} onClick={save}>
            {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
            Make default
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
