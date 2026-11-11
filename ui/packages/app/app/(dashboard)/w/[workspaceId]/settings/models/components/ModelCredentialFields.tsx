import { Button, Input, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@agentsfleet/design-system";
import type { Secret } from "@/lib/api/secrets";
import { OPENAI_COMPATIBLE_PROVIDER } from "@/lib/types";
import { compatibleCredentials, KEY_MODE, type ModelFormValues } from "./model-creation";

type Props = {
  id: string;
  values: ModelFormValues;
  secrets: readonly Secret[];
  onChange: (patch: Partial<ModelFormValues>) => void;
};

export function ModelCredentialFields({id, values, secrets, onChange}: Props) {
  const available = compatibleCredentials(secrets, values.provider);
  return (
    <div className="space-y-lg">
      <p id={`${id}-credential-label`} className="text-label font-control">Provider API key</p>
      <fieldset aria-labelledby={`${id}-credential-label`} className="m-0 grid min-w-0 grid-cols-2 gap-sm rounded-sm border border-border p-sm">
        <Button type="button" variant={values.mode === KEY_MODE.new ? "secondary" : "ghost"} aria-pressed={values.mode === KEY_MODE.new}
          onClick={() => onChange({mode: KEY_MODE.new, apiKey: "", savedRef: ""})}>Add new key</Button>
        <Button type="button" variant={values.mode === KEY_MODE.saved ? "secondary" : "ghost"} aria-pressed={values.mode === KEY_MODE.saved}
          onClick={() => onChange({mode: KEY_MODE.saved, apiKey: ""})}>Use saved key</Button>
      </fieldset>
      {values.mode === KEY_MODE.saved
        ? <SavedCredential id={id} values={values} available={available} onChange={onChange} />
        : <NewCredential id={id} values={values} secrets={secrets} onChange={onChange} />}
    </div>
  );
}

function SavedCredential({id, values, available, onChange}: Omit<Props, "secrets"> & {available: Secret[]}) {
  const selected = available.find(secret => secret.name === values.savedRef);
  return (
    <div className="space-y-sm">
      <Label htmlFor={`${id}-saved-key`}>Saved key</Label>
      <Select value={values.savedRef} onValueChange={savedRef => onChange({savedRef})} disabled={available.length === 0}>
        <SelectTrigger id={`${id}-saved-key`} aria-label="Saved key"><SelectValue placeholder="Select a saved key" /></SelectTrigger>
        <SelectContent>{available.map(secret => <SelectItem key={secret.name} value={secret.name}>{secret.name}</SelectItem>)}</SelectContent>
      </Select>
      <p className="text-eyebrow leading-eyebrow text-muted-foreground">
        {available.length ? "Only keys for this provider are listed. The saved value stays hidden." : "No saved keys for this provider. Add a new key to continue."}
      </p>
      {selected && "base_url" in selected ? <p className="break-words text-eyebrow text-muted-foreground">Endpoint: {selected.base_url}</p> : null}
    </div>
  );
}

function NewCredential({id, values, secrets, onChange}: Props) {
  const custom = values.provider.trim() === OPENAI_COMPATIBLE_PROVIDER;
  const held = values.keyName.trim() !== "" && secrets.some(secret => secret.name === values.keyName.trim());
  return (
    <div className="space-y-lg">
      <div className="space-y-sm">
        <Label htmlFor={`${id}-key-name`}>Key name</Label>
        <Input id={`${id}-key-name`} value={values.keyName} onChange={event => onChange({keyName: event.target.value})} placeholder="OpenAI workspace" autoComplete="off" spellCheck={false} aria-invalid={held || undefined} />
        {held ? <output className="block text-eyebrow text-warning">That name is already in use. Choose Use saved key or enter another name.</output> : null}
      </div>
      {custom ? <div className="space-y-sm">
        <Label htmlFor={`${id}-base-url`}>Base URL</Label>
        <Input id={`${id}-base-url`} value={values.baseUrl} onChange={event => onChange({baseUrl: event.target.value})} placeholder="https://your-provider.example/v1" autoComplete="off" spellCheck={false} />
        <p className="text-eyebrow text-muted-foreground">Use an HTTPS OpenAI-compatible endpoint.</p>
      </div> : null}
      <div className="space-y-sm">
        <Label htmlFor={`${id}-api-key`}>{custom ? "API key (optional)" : "API key"}</Label>
        <Input id={`${id}-api-key`} type="password" value={values.apiKey} onChange={event => onChange({apiKey: event.target.value})}
          autoComplete="off" spellCheck={false} placeholder={custom ? "Leave blank if the endpoint needs no key" : "Enter your provider key"} />
        <p className="text-eyebrow text-muted-foreground">The key is saved securely and its value stays hidden.</p>
      </div>
    </div>
  );
}
