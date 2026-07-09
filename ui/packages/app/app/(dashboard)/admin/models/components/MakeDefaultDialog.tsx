"use client";

import { useState, useTransition } from "react";
import {
  Badge,
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
import { type AdminModel, OPENAI_COMPATIBLE_PROVIDER } from "@/lib/api/admin_models";
import { presentErrorString } from "@/lib/errors";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { setPlatformDefaultAction } from "../actions";

const LABEL = "block text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-1.5";

// Minimal "make this row the platform default" dialog: provider + model are the
// row's known identity, so it collects ONLY the provider API key (plus a base URL
// for the openai-compatible provider). Reuses setPlatformDefaultAction — the key
// is written to the acting admin's vault server-side and never echoed back, which
// is why a re-set always re-enters it. Mounted only while a row is targeted
// (parent keys it by uid), so it opens clean each time.
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
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const canSave = apiKey !== "" && (!isCustom || baseUrl !== "");

  function save() {
    setApiError(null);
    startTransition(async () => {
      const r = await setPlatformDefaultAction({
        provider: model.provider,
        model: model.model_id,
        api_key: apiKey,
        base_url: isCustom ? baseUrl : undefined,
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
          <DialogDescription>
            Every fleet runs <Badge variant="cyan">{model.provider}</Badge>{" "}
            <span className="font-mono">{model.model_id}</span> when a user hasn&apos;t brought their own key,
            billed at its catalogue rate. Enter the provider API key — it&apos;s stored in your vault and
            never shown again.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div>
            <label className={LABEL} htmlFor="md-key">API key</label>
            <Input
              id="md-key"
              type="password"
              autoComplete="off"
              placeholder="stored in your workspace vault; never shown again"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
            />
          </div>

          {isCustom ? (
            <div>
              <label className={LABEL} htmlFor="md-base-url">Base URL</label>
              <Input
                id="md-base-url"
                placeholder="https://…/v1"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
              />
              <p className="mt-1 text-xs text-muted-foreground">Required for an OpenAI-compatible endpoint (https only).</p>
            </div>
          ) : null}

          {apiError ? <p className="text-sm text-destructive">{apiError}</p> : null}
        </div>

        <DialogFooter>
          <Button type="button" disabled={!canSave || pending} onClick={save}>
            {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
            Make default
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
