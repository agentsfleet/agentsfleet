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
import { createModelEntryAction, replaceSecretAction, setProviderSelfManagedAction } from "../actions";
import { isHttpsUrl, BASE_URL_NOT_HTTPS } from "../lib/custom-endpoint";
import { presentErrorString } from "@/lib/errors";
import { SECRET_KIND, type Secret } from "@/lib/api/secrets";
import { providerLabel, uniqueProviders } from "@/lib/api/model_library";
import { OPENAI_COMPATIBLE_PROVIDER, SECRET_FIELD } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { maySpeculateOnHover } from "@/components/domain/island-dynamic/intent-module-loader";
import { captureModelActivated } from "../lib/track";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import ProviderModelSelect from "./ProviderModelSelect";
import { SECRETS_LOAD, type SecretsLoad } from "./secrets-load";
import { requestOnboardingRefresh } from "@/lib/onboarding-refresh";

const REGISTER_ACTION = "register the model entry";
const ACTIVATE_ACTION = "activate this model";
const STORE_ACTION = "store the credential";
const NAME_PROVIDER_MISMATCH = "That name is already used by a different provider or secret — pick another one.";
const CREATE_MODEL_TOOLTIP = "Create a model entry for this workspace.";
const SECRETS_LOADING = "Checking your stored secrets…";
const SECRETS_LOAD_FAILED =
  "Couldn't load your stored secrets. Saving is disabled so an existing secret can't be silently overwritten.";

export default function AddModelEntryDialog({
  workspaceId,
  secrets,
  secretsLoad,
  onCreated,
  onSecretsChanged,
  onSecretsNeeded,
}: {
  workspaceId: string;
  secrets: Secret[];
  /**
   * Whether `secrets` above is trustworthy yet. Anything but `ready` disables
   * Save: submit() resolves rotate-vs-create and the name-ownership guard
   * from that list, and the secrets POST upserts — an unloaded list would
   * silently overwrite whatever already holds the typed name.
   */
  secretsLoad: SecretsLoad;
  onCreated: () => void;
  onSecretsChanged: () => void;
  /** Load the stored-secret list. Fired on open — see handleOpenChange. */
  onSecretsNeeded: () => void;
}) {
  const uid = useId();
  const { models, status: catalogueStatus, preload } = useModelCatalogue();
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
  //
  // The secrets-load arm is a safety gate, not polish: firing the load on
  // open is worthless if a fast hand can submit before it lands, and the
  // window is not only a race — a failed load would leave the list empty for
  // the whole session. Either way `existing` would resolve to undefined and
  // the create path's upsert would stomp the stored secret unseen.
  const canSubmit =
    secretsLoad === SECRETS_LOAD.ready &&
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
    if (next) {
      // Opening is the strongest intent signal there is, and it is not gated
      // on pointer or data policy: the picker inside needs the catalogue now.
      // A hover or focus has usually warmed it already, so this is normally a
      // no-op against the single-flight guard.
      preload();
      // The stored-secret list is LOAD-BEARING, not decoration: `submit()`
      // resolves `existing` from it to decide rotate-vs-create and to refuse a
      // name owned by a different provider. The secrets POST is an upsert
      // server-side, so an empty list here does not degrade to a picker with
      // no options — it silently overwrites whatever already holds that name.
      // It must be loaded before the dialog can be submitted, not merely after
      // a secret changes.
      onSecretsNeeded();
    }
    setOpen(next);
    if (!next) reset();
  }

  // `secretsChanged` is false on the rotate branch — a rotate keeps the
  // secret's list-visible metadata (name/provider/kind) identical, so the
  // refetch would return the same data.
  async function doCreateEntry(secretRef: string, modelId: string, activate: boolean, secretsChanged: boolean) {
    const created = await createModelEntryAction({ model_id: modelId, secret_ref: secretRef });
    if (!created.ok) {
      if (secretsChanged) requestOnboardingRefresh(workspaceId);
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
        if (secretsChanged) requestOnboardingRefresh(workspaceId);
        setError(presentErrorString({ errorCode: activated.errorCode, message: activated.error, action: ACTIVATE_ACTION }));
        return false;
      }
      captureModelActivated(activated.data);
    }
    if (secretsChanged || activate) requestOnboardingRefresh(workspaceId);
    return true;
  }

  /** Store or replace the credential, register the entry, optionally activate.
   * Name is the credential's identity, guarded across EVERY stored kind: a
   * name owned by a different shape errors instead of being replaced. A
   * same-shape held name is replaced whole via PUT — create claims free
   * names only and answers UZ-VAULT-005 on a held one. */
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
      if (existing) {
        // A held endpoint reconfigures via whole-body replace, never create.
        const replaced = await replaceSecretAction(workspaceId, name, data);
        if (!replaced.ok) {
          setError(presentErrorString({ errorCode: replaced.errorCode, message: replaced.error, action: STORE_ACTION }));
          return;
        }
        if (await doCreateEntry(name, modelId, activate, false)) handleOpenChange(false);
        return;
      }
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
      const replaced = await replaceSecretAction(workspaceId, name, { [SECRET_FIELD.provider]: provider.trim(), [SECRET_FIELD.apiKey]: key });
      if (!replaced.ok) {
        setError(presentErrorString({ errorCode: replaced.errorCode, message: replaced.error, action: STORE_ACTION }));
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
        <TooltipButton
          type="button"
          size="sm"
          className="gap-1.5"
          tooltip={CREATE_MODEL_TOOLTIP}
          // Focus is deliberate — keyboard users get the same warm dialog a
          // mouse user gets from hovering, and it is never suppressed.
          onFocus={preload}
          // Hover only speculates where hover means something, the user has
          // not asked us to conserve data, and the catalogue is not already
          // known-failing — mousing around a failing backend must not fire a
          // request per hover. Open still retries deliberately.
          onPointerEnter={() => {
            if (catalogueStatus !== CATALOGUE_STATUS.error && maySpeculateOnHover()) preload();
          }}
        >
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
        {/* Why Save is disabled, said out loud. `<output>` carries an implicit
            status role, so the loading line announces without an explicit
            attribute; the failed load is an alert with its own retry, wired
            to the same load the open fired. */}
        {secretsLoad === SECRETS_LOAD.error ? (
          <Alert variant="destructive" className="flex items-center gap-3 text-xs">
            {SECRETS_LOAD_FAILED}
            <Button type="button" variant="outline" size="sm" onClick={onSecretsNeeded}>
              Retry
            </Button>
          </Alert>
        ) : secretsLoad !== SECRETS_LOAD.ready ? (
          <output className="block text-xs text-muted-foreground">{SECRETS_LOADING}</output>
        ) : null}
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
