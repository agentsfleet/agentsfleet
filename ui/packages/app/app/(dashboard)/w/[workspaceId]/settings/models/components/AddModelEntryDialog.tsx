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
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import { createSecretAction } from "@/app/(dashboard)/w/[workspaceId]/secrets/actions";
import { createModelEntryAction, setProviderSelfManagedAction } from "../actions";
import { detectProviderFromKey } from "../lib/detect-provider";
import { isHttpsUrl, BASE_URL_NOT_HTTPS } from "../lib/custom-endpoint";
import { presentErrorString } from "@/lib/errors";
import { providerKeysOf, type Secret } from "@/lib/api/secrets";
import { providerLabel, uniqueProviders } from "@/lib/api/model_caps";
import { OPENAI_COMPATIBLE_PROVIDER, SECRET_FIELD } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { captureModelActivated } from "../lib/track";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import ProviderModelSelect from "./ProviderModelSelect";

const SHAPE = { known: "known", custom: "custom" } as const;
const REGISTER_ACTION = "register the model entry";
const ACTIVATE_ACTION = "activate this model";
const STORE_ACTION = "store the credential";
const STALE_KEY_ERROR = "That stored key is no longer available — pick another.";

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
  const providerOptions = uniqueProviders(models);
  const providerKeys = providerKeysOf(secrets);

  const [open, setOpen] = useState(false);
  const [shape, setShape] = useState<(typeof SHAPE)[keyof typeof SHAPE]>(SHAPE.known);
  const [reuseMode, setReuseMode] = useState(false);
  const [reuseSecretName, setReuseSecretName] = useState("");
  const [keyName, setKeyName] = useState("");
  const [provider, setProvider] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [customName, setCustomName] = useState("");
  const [customBaseUrl, setCustomBaseUrl] = useState("");
  const [customApiKey, setCustomApiKey] = useState("");
  const [customModel, setCustomModel] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Gates both Save buttons below — without it, a click on an incomplete
  // form silently no-ops (no error, no feedback) since submitKnown/
  // submitCustom validate internally. Matches EditModelEntryDialog's
  // `disabled={pending || !canSubmit}` convention.
  const canSubmitKnown = reuseMode
    ? reuseSecretName.trim() !== "" && model.trim() !== ""
    : keyName.trim() !== "" && provider.trim() !== "" && apiKey.trim() !== "" && model.trim() !== "";
  const canSubmitCustom = customName.trim() !== "" && customBaseUrl.trim() !== "" && customModel.trim() !== "";
  const canSubmit = shape === SHAPE.known ? canSubmitKnown : canSubmitCustom;

  function reset() {
    setShape(SHAPE.known);
    setReuseMode(false);
    setReuseSecretName("");
    setKeyName("");
    setProvider("");
    setApiKey("");
    setModel("");
    setCustomName("");
    setCustomBaseUrl("");
    setCustomApiKey("");
    setCustomModel("");
    setError(null);
  }

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (!next) reset();
  }

  function onApiKeyChange(value: string) {
    setApiKey(value);
    const detected = detectProviderFromKey(value);
    if (detected && detected !== provider) {
      setProvider(detected);
      setKeyName(detected);
      setModel("");
    }
  }

  async function doCreateEntry(secretRef: string, modelId: string, activate: boolean) {
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
    onSecretsChanged();
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

  async function submitKnown(activate: boolean) {
    if (reuseMode) {
      // canSubmitKnown only guarantees reuseSecretName is non-empty, not that
      // it still names a stored key — a secrets refresh after a create can
      // change `secrets` (and so providerKeys) out from under an open dialog.
      const secret = providerKeys.find((k) => k.name === reuseSecretName);
      if (!secret) {
        setError(STALE_KEY_ERROR);
        return;
      }
      if (await doCreateEntry(secret.name, model.trim(), activate)) finish();
      return;
    }
    const name = keyName.trim();
    const created = await createSecretAction(workspaceId, {
      name,
      data: { [SECRET_FIELD.provider]: provider.trim(), [SECRET_FIELD.apiKey]: apiKey.trim() },
    });
    if (!created.ok) {
      setError(presentErrorString({ errorCode: created.errorCode, message: created.error, action: STORE_ACTION }));
      return;
    }
    captureProductEvent(EVENTS.secret_added, { secret_name: name });
    if (await doCreateEntry(name, model.trim(), activate)) finish();
  }

  async function submitCustom(activate: boolean) {
    const name = customName.trim();
    const modelId = customModel.trim();
    if (!isHttpsUrl(customBaseUrl)) {
      setError(BASE_URL_NOT_HTTPS);
      return;
    }
    const data: Record<string, unknown> = {
      [SECRET_FIELD.provider]: OPENAI_COMPATIBLE_PROVIDER,
      [SECRET_FIELD.baseUrl]: customBaseUrl.trim(),
    };
    const key = customApiKey.trim();
    if (key !== "") data[SECRET_FIELD.apiKey] = key;
    const created = await createSecretAction(workspaceId, { name, data });
    if (!created.ok) {
      setError(presentErrorString({ errorCode: created.errorCode, message: created.error, action: STORE_ACTION }));
      return;
    }
    captureProductEvent(EVENTS.secret_added, { secret_name: name });
    if (await doCreateEntry(name, modelId, activate)) finish();
  }

  // Only reached after doCreateEntry() returns true — the refresh (entries +
  // secrets) already happened there, unconditionally, the moment the entry
  // itself committed. This just closes the dialog on full success.
  function finish() {
    handleOpenChange(false);
  }

  // Only wired to the Save / Save & make active buttons below, both
  // disabled whenever `pending` — no redundant re-check needed here.
  async function onSubmit(activate: boolean) {
    setError(null);
    setPending(true);
    try {
      if (shape === SHAPE.known) await submitKnown(activate);
      else await submitCustom(activate);
    } finally {
      setPending(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button type="button" size="sm" className="gap-1.5">
          <PlusIcon size={14} />
          Add model
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add a model</DialogTitle>
          <DialogDescription>Register a model entry. A key is only asked for once per stored credential.</DialogDescription>
        </DialogHeader>
        <Tabs value={shape} onValueChange={(v) => setShape(v as typeof shape)}>
          <TabsList>
            <TabsTrigger value={SHAPE.known}>Known provider</TabsTrigger>
            <TabsTrigger value={SHAPE.custom}>Custom endpoint</TabsTrigger>
          </TabsList>
          <TabsContent value={SHAPE.known} className="space-y-3">
            {providerKeys.length > 0 ? (
              <div className="flex gap-2">
                <Button type="button" size="sm" variant={reuseMode ? "outline" : "default"} onClick={() => setReuseMode(false)}>
                  New key
                </Button>
                <Button type="button" size="sm" variant={reuseMode ? "default" : "outline"} onClick={() => setReuseMode(true)}>
                  Use existing key
                </Button>
              </div>
            ) : null}
            {reuseMode ? (
              <div className="space-y-2">
                <Label htmlFor={`${uid}-reuse`}>Stored key</Label>
                <Select value={reuseSecretName} onValueChange={(v) => { setReuseSecretName(v); setModel(""); }}>
                  <SelectTrigger id={`${uid}-reuse`} aria-label="Stored key">
                    <SelectValue placeholder="Select a stored key" />
                  </SelectTrigger>
                  <SelectContent>
                    {providerKeys.map((k) => (
                      <SelectItem key={k.name} value={k.name}>{providerLabel(k.provider)} — {k.name}</SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <ProviderModelSelect
                  id={`${uid}-reuse-model`}
                  provider={providerKeys.find((k) => k.name === reuseSecretName)?.provider}
                  model={model}
                  onModelChange={setModel}
                />
              </div>
            ) : (
              <div className="space-y-3">
                <div className="space-y-2">
                  <Label htmlFor={`${uid}-api-key`}>API key</Label>
                  <Input id={`${uid}-api-key`} type="password" value={apiKey} onChange={(e) => onApiKeyChange(e.target.value)} placeholder="paste your key — we'll detect common providers" spellCheck={false} autoComplete="off" />
                </div>
                <div className="space-y-2">
                  <Label htmlFor={`${uid}-provider`}>Provider</Label>
                  {providerOptions.length > 0 ? (
                    <Select value={provider} onValueChange={(v) => { setProvider(v); setKeyName(v); setModel(""); }}>
                      <SelectTrigger id={`${uid}-provider`} aria-label="Provider">
                        <SelectValue placeholder="Select a provider" />
                      </SelectTrigger>
                      <SelectContent>
                        {providerOptions.map((p) => <SelectItem key={p} value={p}>{providerLabel(p)}</SelectItem>)}
                      </SelectContent>
                    </Select>
                  ) : (
                    <Input id={`${uid}-provider`} value={provider} onChange={(e) => { setProvider(e.target.value); setKeyName(e.target.value); setModel(""); }} placeholder="anthropic" spellCheck={false} autoComplete="off" />
                  )}
                </div>
                <div className="space-y-2">
                  <Label htmlFor={`${uid}-key-name`}>Key name</Label>
                  <Input id={`${uid}-key-name`} value={keyName} onChange={(e) => setKeyName(e.target.value)} placeholder="anthropic-prod" spellCheck={false} autoComplete="off" />
                </div>
                <ProviderModelSelect id={`${uid}-model`} provider={provider || undefined} model={model} onModelChange={setModel} />
              </div>
            )}
          </TabsContent>
          <TabsContent value={SHAPE.custom} className="space-y-3">
            <div className="space-y-2">
              <Label htmlFor={`${uid}-c-name`}>Name</Label>
              <Input id={`${uid}-c-name`} value={customName} onChange={(e) => setCustomName(e.target.value)} placeholder="vllm-gateway" spellCheck={false} autoComplete="off" />
            </div>
            <div className="space-y-2">
              <Label htmlFor={`${uid}-c-url`}>Base URL</Label>
              <Input id={`${uid}-c-url`} value={customBaseUrl} onChange={(e) => setCustomBaseUrl(e.target.value)} placeholder="https://vllm.corp/v1" spellCheck={false} autoComplete="off" />
              <p className="text-xs text-muted-foreground">Any OpenAI-compatible endpoint. Must use https; loopback and private hosts are rejected.</p>
            </div>
            <div className="space-y-2">
              <Label htmlFor={`${uid}-c-key`}>API key (optional)</Label>
              <Input id={`${uid}-c-key`} type="password" value={customApiKey} onChange={(e) => setCustomApiKey(e.target.value)} placeholder="leave blank if the endpoint needs no key" spellCheck={false} autoComplete="off" />
            </div>
            <ProviderModelSelect id={`${uid}-c-model`} provider={OPENAI_COMPATIBLE_PROVIDER} model={customModel} onModelChange={setCustomModel} />
          </TabsContent>
        </Tabs>
        {error ? <Alert variant="destructive" className="text-xs">{error}</Alert> : null}
        <DialogFooter>
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
