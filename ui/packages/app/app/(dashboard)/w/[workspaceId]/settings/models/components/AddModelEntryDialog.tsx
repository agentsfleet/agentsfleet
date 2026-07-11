"use client";

import { useId, useState } from "react";
import {
  Alert,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Spinner,
  TooltipButton,
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import { createSecretAction } from "@/app/(dashboard)/w/[workspaceId]/secrets/actions";
import { createModelEntryAction, rotateSecretAction, setProviderSelfManagedAction } from "../actions";
import { isHttpsUrl, BASE_URL_NOT_HTTPS } from "../lib/custom-endpoint";
import { presentErrorString } from "@/lib/errors";
import { SECRET_KIND, type Secret } from "@/lib/api/secrets";
import { providerLabel, uniqueProviders } from "@/lib/api/model_library";
import { OPENAI_COMPATIBLE_PROVIDER, SECRET_FIELD } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { captureModelActivated } from "../lib/track";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import ProviderModelSelect from "./ProviderModelSelect";

const REGISTER_ACTION = "register the model entry";
const ACTIVATE_ACTION = "activate this model";
const STORE_ACTION = "store the credential";
const NAME_PROVIDER_MISMATCH = "That name is already used by a different provider or secret — pick another one.";
const CREATE_MODEL_TOOLTIP = "Create a model entry for this workspace.";

export default function AddModelEntryDialog({
  workspaceId,
  secrets,
  onCreated,
  onSecretsChanged,
}: {
  workspaceId: string;
  secrets: Secret[];
  onCreated: () => void;
  onSecretsChanged: () => void;
}) {
  const uid = useId();
  const { models } = useModelCatalogue();
  // The library's providers plus the OpenAI-compatible option, pinned last —
  // one dropdown covers hosted providers and custom endpoints alike (no tabs).
  const providerOptions = uniqueProviders(models).filter((p) => p !== OPENAI_COMPATIBLE_PROVIDER);

  const [open, setOpen] = useState(false);
  const [keyName, setKeyName] = useState("");
  const [provider, setProvider] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isCustom = provider.trim() === OPENAI_COMPATIBLE_PROVIDER;

  // Gates both Save buttons below — without it, a click on an incomplete
  // form silently no-ops (no error, no feedback) since submit() validates
  // internally. A custom endpoint may be keyless; a named provider never is.
  const canSubmit =
    keyName.trim() !== "" &&
    provider.trim() !== "" &&
    model.trim() !== "" &&
    (isCustom ? baseUrl.trim() !== "" : apiKey.trim() !== "");

  function reset() {
    setKeyName("");
    setProvider("");
    setBaseUrl("");
    setModel("");
    setApiKey("");
    setError(null);
  }

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (!next) reset();
  }

  // `secretsChanged` is false on the rotate branch — a rotate keeps the
  // secret's list-visible metadata (name/provider/kind) identical, so the
  // refetch would return the same data.
  async function doCreateEntry(secretRef: string, modelId: string, activate: boolean, secretsChanged: boolean) {
    const created = await createModelEntryAction({ model_id: modelId, secret_ref: secretRef });
    if (!created.ok) {
      setError(presentErrorString({ errorCode: created.errorCode, message: created.error, action: REGISTER_ACTION }));
      return false;
    }
    // The entry is committed server-side from here on regardless of what
    // activation does next — refresh now so a retry after an activation
    // failure never re-POSTs the same (model_id, secret_ref) the user never
    // saw succeed (that retry would 409 UZ-MODELS-003 "duplicate entry"),
    // and the table isn't stale if the user cancels instead of retrying.
    onCreated();
    if (secretsChanged) onSecretsChanged();
    if (activate) {
      const activated = await setProviderSelfManagedAction({ secret_ref: secretRef, model: modelId });
      if (!activated.ok) {
        setError(presentErrorString({ errorCode: activated.errorCode, message: activated.error, action: ACTIVATE_ACTION }));
        return false;
      }
      captureModelActivated(activated.data);
    }
    return true;
  }

  /** Store (or rotate) the credential, register the entry, optionally activate.
   * Name is the credential's identity, guarded across EVERY stored kind: a
   * name owned by anything other than a same-shaped secret errors instead of
   * overwriting (the secrets POST is an upsert server-side, so a silent
   * collision would destroy the original credential's body). Reusing a name
   * with the SAME named provider rotates the api_key in place; reusing an
   * OpenAI-compatible name while the compatible provider is selected rewrites
   * the endpoint's base_url + key together (the custom reconfigure motion). */
  async function submit(activate: boolean) {
    const name = keyName.trim();
    const modelId = model.trim();
    const key = apiKey.trim();
    const existing = secrets.find((s) => s.name === name);

    if (isCustom) {
      if (existing && existing.kind !== SECRET_KIND.custom_endpoint) {
        setError(NAME_PROVIDER_MISMATCH);
        return;
      }
      if (!isHttpsUrl(baseUrl)) {
        setError(BASE_URL_NOT_HTTPS);
        return;
      }
      const data: Record<string, unknown> = {
        [SECRET_FIELD.provider]: OPENAI_COMPATIBLE_PROVIDER,
        [SECRET_FIELD.baseUrl]: baseUrl.trim(),
      };
      if (key !== "") data[SECRET_FIELD.apiKey] = key;
      const created = await createSecretAction(workspaceId, { name, data });
      if (!created.ok) {
        setError(presentErrorString({ errorCode: created.errorCode, message: created.error, action: STORE_ACTION }));
        return;
      }
      captureProductEvent(EVENTS.secret_added, { secret_name: name });
      if (await doCreateEntry(name, modelId, activate, true)) handleOpenChange(false);
      return;
    }

    if (existing) {
      if (existing.kind !== SECRET_KIND.provider_key || existing.provider !== provider.trim()) {
        setError(NAME_PROVIDER_MISMATCH);
        return;
      }
      const rotated = await rotateSecretAction(workspaceId, name, key);
      if (!rotated.ok) {
        setError(presentErrorString({ errorCode: rotated.errorCode, message: rotated.error, action: STORE_ACTION }));
        return;
      }
      if (await doCreateEntry(name, modelId, activate, false)) handleOpenChange(false);
      return;
    }
    const created = await createSecretAction(workspaceId, {
      name,
      data: { [SECRET_FIELD.provider]: provider.trim(), [SECRET_FIELD.apiKey]: key },
    });
    if (!created.ok) {
      setError(presentErrorString({ errorCode: created.errorCode, message: created.error, action: STORE_ACTION }));
      return;
    }
    captureProductEvent(EVENTS.secret_added, { secret_name: name });
    if (await doCreateEntry(name, modelId, activate, true)) handleOpenChange(false);
  }

  // Only wired to the Save / Save & make active buttons below, both
  // disabled whenever `pending` — no redundant re-check needed here.
  async function onSubmit(activate: boolean) {
    setError(null);
    setPending(true);
    try {
      await submit(activate);
    } finally {
      setPending(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <TooltipButton type="button" size="sm" className="gap-1.5" tooltip={CREATE_MODEL_TOOLTIP}>
          <PlusIcon size={14} />
          Create model
        </TooltipButton>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create model</DialogTitle>
          <DialogDescription>Store the key and register a model your fleets can use.</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div className="space-y-2">
            <Label htmlFor={`${uid}-name`}>Name</Label>
            <Input id={`${uid}-name`} value={keyName} onChange={(e) => setKeyName(e.target.value)} placeholder="anthropic-prod" spellCheck={false} autoComplete="off" />
          </div>
          <div className="space-y-2">
            <Label htmlFor={`${uid}-provider`}>Provider</Label>
            {providerOptions.length > 0 ? (
              <Select value={provider} onValueChange={(v) => { setProvider(v); setModel(""); }}>
                <SelectTrigger id={`${uid}-provider`} aria-label="Provider">
                  <SelectValue placeholder="Select a provider" />
                </SelectTrigger>
                <SelectContent>
                  {providerOptions.map((p) => <SelectItem key={p} value={p}>{providerLabel(p)}</SelectItem>)}
                  <SelectItem value={OPENAI_COMPATIBLE_PROVIDER}>{providerLabel(OPENAI_COMPATIBLE_PROVIDER)}</SelectItem>
                </SelectContent>
              </Select>
            ) : (
              // Library unavailable (fetch failed / empty) — degrade to free
              // text so a key can still be stored; typing the compatible
              // provider id reveals the Base URL field the same way.
              <Input id={`${uid}-provider`} value={provider} onChange={(e) => { setProvider(e.target.value); setModel(""); }} placeholder="anthropic" spellCheck={false} autoComplete="off" />
            )}
          </div>
          {isCustom ? (
            <div className="space-y-2">
              <Label htmlFor={`${uid}-base-url`}>Base URL</Label>
              <Input id={`${uid}-base-url`} value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://vllm.corp/v1" spellCheck={false} autoComplete="off" />
              <p className="text-xs text-muted-foreground">Any OpenAI-compatible endpoint. Must use https; loopback and private hosts are rejected.</p>
            </div>
          ) : null}
          <ProviderModelSelect id={`${uid}-model`} provider={provider || undefined} model={model} onModelChange={setModel} />
          <div className="space-y-2">
            <Label htmlFor={`${uid}-api-key`}>{isCustom ? "API key (optional)" : "API key"}</Label>
            <Input
              id={`${uid}-api-key`}
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={isCustom ? "leave blank if the endpoint needs no key" : "stored in your workspace vault; never shown again"}
              spellCheck={false}
              autoComplete="off"
            />
          </div>
        </div>
        {error ? <Alert variant="destructive" className="text-xs">{error}</Alert> : null}
        <DialogFooter>
          <Button type="button" variant="ghost" disabled={pending} onClick={() => handleOpenChange(false)}>
            Cancel
          </Button>
          <Button type="button" variant="outline" disabled={pending || !canSubmit} onClick={() => void onSubmit(false)}>
            {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
            Save
          </Button>
          <Button type="button" disabled={pending || !canSubmit} onClick={() => void onSubmit(true)}>
            {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
            Save & make active
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
