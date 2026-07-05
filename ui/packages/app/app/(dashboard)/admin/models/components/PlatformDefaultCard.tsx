"use client";

import { useMemo, useState, useTransition } from "react";
import {
  Badge,
  Button,
  Card,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Spinner,
} from "@agentsfleet/design-system";
import { type AdminModel, OPENAI_COMPATIBLE_PROVIDER } from "@/lib/api/admin_models";
import { presentErrorString } from "@/lib/errors";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";
import { setPlatformDefaultAction } from "../actions";

const LABEL = "block text-xs font-semibold uppercase tracking-wider text-muted-foreground mb-1.5";

// The model options come ONLY from the catalogue (filtered by provider), so a
// platform default that isn't a priced row is unselectable here — the billing
// spine is enforced in the UI by construction, and again server-side.
export default function PlatformDefaultCard({ models }: { models: AdminModel[] }) {
  const providers = useMemo(() => Array.from(new Set(models.map((m) => m.provider))).sort(), [models]);
  const [provider, setProvider] = useState<string>("");
  const [model, setModel] = useState<string>("");
  const [apiKey, setApiKey] = useState<string>("");
  const [baseUrl, setBaseUrl] = useState<string>("");
  const [pending, startTransition] = useTransition();
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const modelsForProvider = useMemo(
    () => models.filter((m) => m.provider === provider),
    [models, provider],
  );
  const isCustom = provider === OPENAI_COMPATIBLE_PROVIDER;
  const canSave = provider !== "" && model !== "" && apiKey !== "" && (!isCustom || baseUrl !== "");

  function save() {
    setError(null);
    setSaved(false);
    startTransition(async () => {
      const r = await setPlatformDefaultAction({
        provider,
        model,
        api_key: apiKey,
        base_url: isCustom ? baseUrl : undefined,
      });
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "set the platform default" }));
        return;
      }
      setApiKey("");
      setSaved(true);
      captureProductEvent(EVENTS.platform_default_set, { provider, model, is_custom: isCustom });
    });
  }

  return (
    <div className="space-y-4">
      <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Platform default</p>
      <Card className="space-y-5 p-5">
        <p className="max-w-xl text-sm text-muted-foreground">
          As a platform admin, set the default model teammates run when they haven&apos;t brought
          their own key. It&apos;s billed at the catalogue rate above; the key you provide stays in
          your vault, so teammates never see it.
        </p>

        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <label className={LABEL} htmlFor="pd-provider">Provider</label>
            <Select
              value={provider}
              onValueChange={(v) => { setProvider(v); setModel(""); }}
            >
              <SelectTrigger id="pd-provider" aria-label="Default provider">
                <SelectValue placeholder="Select a provider" />
              </SelectTrigger>
              <SelectContent>
                {providers.map((p) => <SelectItem key={p} value={p}>{p}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <div>
            <label className={LABEL} htmlFor="pd-model">Model</label>
            <Select value={model} onValueChange={setModel} disabled={provider === ""}>
              <SelectTrigger id="pd-model" aria-label="Default model">
                <SelectValue placeholder="Select a catalogued model" />
              </SelectTrigger>
              <SelectContent>
                {modelsForProvider.map((m) => <SelectItem key={m.uid} value={m.model_id}>{m.model_id}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
        </div>

        <div>
          <label className={LABEL} htmlFor="pd-key">API key</label>
          <Input
            id="pd-key"
            type="password"
            autoComplete="off"
            className="font-mono"
            placeholder="stored in your workspace vault; never shown again"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
          />
        </div>

        {isCustom ? (
          <div>
            <label className={LABEL} htmlFor="pd-base-url">Base URL</label>
            <Input
              id="pd-base-url"
              className="font-mono"
              placeholder="https://…/v1"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
            />
            <p className="mt-1 text-xs text-muted-foreground">Required for an OpenAI-compatible endpoint (https only).</p>
          </div>
        ) : null}

        {error ? <p className="text-sm text-destructive">{error}</p> : null}
        {saved ? <p className="text-sm text-success">Platform default updated.</p> : null}

        <div className="flex items-center justify-between border-t border-border pt-4">
          <span className="text-xs text-muted-foreground">
            {provider && model ? (
              <>Will activate <Badge variant="cyan">{provider}</Badge> <span className="font-mono">{model}</span></>
            ) : "Select a provider and model"}
          </span>
          <Button type="button" disabled={!canSave || pending} onClick={save}>
            {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
            Save default
          </Button>
        </div>
      </Card>
    </div>
  );
}
