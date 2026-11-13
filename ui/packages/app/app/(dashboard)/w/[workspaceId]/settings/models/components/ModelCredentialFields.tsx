import { RadioGroup, RadioGroupItem, Input, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@agentsfleet/design-system";
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
      <p id={`${id}-credential-label`} className="text-label font-semibold">Provider API key</p>
      <RadioGroup aria-labelledby={`${id}-credential-label`} value={values.mode}
        onValueChange={mode => onChange({mode: mode === KEY_MODE.saved ? KEY_MODE.saved : KEY_MODE.new, apiKey: "", savedRef: ""})}
        className="grid min-h-13.5 grid-cols-2 gap-sm rounded-control border border-border bg-background p-sm">
        {[{mode: KEY_MODE.new, label: "Add new key"}, {mode: KEY_MODE.saved, label: "Use saved key"}].map(option =>
          <Label key={option.mode} htmlFor={`${id}-${option.mode}`} className={`m-0 flex cursor-pointer items-center justify-center gap-md rounded-control px-md py-lg text-label ${values.mode === option.mode ? "bg-muted" : ""}`}>
            <RadioGroupItem id={`${id}-${option.mode}`} value={option.mode} />{option.label}
          </Label>)}
      </RadioGroup>
      {values.mode === KEY_MODE.saved
        ? <SavedCredential id={id} values={values} available={available} onChange={onChange} />
        : <NewCredential id={id} values={values} secrets={secrets} onChange={onChange} />}
    </div>
  );
}

function SavedCredential({id, values, available, onChange}: Omit<Props, "secrets"> & {available: Secret[]}) {
  const selected = available.find(secret => secret.name === values.savedRef);
  return (
    <div className="space-y-md">
      <Label htmlFor={`${id}-saved-key`}>Saved key</Label>
      <Select value={values.savedRef} onValueChange={savedRef => onChange({savedRef})} disabled={available.length === 0}>
        <SelectTrigger id={`${id}-saved-key`} aria-label="Saved key"><SelectValue placeholder="Select a saved key" /></SelectTrigger>
        <SelectContent>{available.map(secret => <SelectItem key={secret.name} value={secret.name}>{secret.name}</SelectItem>)}</SelectContent>
      </Select>
      <p className="text-eyebrow leading-eyebrow text-text-dim">
        {!values.provider.trim() ? "Choose a provider to see its saved keys." : available.length ? "Only keys for this provider are listed. The saved value stays hidden." : "No saved keys for this provider. Add a new key to continue."}
      </p>
      {selected && "base_url" in selected ? <p className="break-words text-eyebrow text-text-dim">Endpoint: {selected.base_url}</p> : null}
    </div>
  );
}

function NewCredential({id, values, secrets, onChange}: Props) {
  const custom = values.provider.trim() === OPENAI_COMPATIBLE_PROVIDER;
  const held = values.keyName.trim() !== "" && secrets.some(secret => secret.name === values.keyName.trim());
  return (
    <div className="space-y-lg">
      <div className="space-y-md">
        <Label htmlFor={`${id}-key-name`}>Key name</Label>
        <Input id={`${id}-key-name`} value={values.keyName} onChange={event => onChange({keyName: event.target.value})} placeholder="Workspace model key" autoComplete="off" spellCheck={false} aria-invalid={held || undefined} />
        <p className="text-eyebrow leading-eyebrow text-text-dim">A name to recognize this key. Its saved value stays hidden.</p>
        {held ? <output className="block text-eyebrow text-warning">That name is already in use. Choose Use saved key or enter another name.</output> : null}
      </div>
      {custom ? <div className="space-y-md">
        <Label htmlFor={`${id}-base-url`}>Base URL</Label>
        <Input id={`${id}-base-url`} value={values.baseUrl} onChange={event => onChange({baseUrl: event.target.value})} placeholder="https://your-provider.example/v1" autoComplete="off" spellCheck={false} />
        <p className="text-eyebrow text-text-dim">Use an HTTPS OpenAI-compatible endpoint.</p>
      </div> : null}
      <div className="space-y-md">
        <Label htmlFor={`${id}-api-key`}>API key</Label>
        <Input id={`${id}-api-key`} type="password" value={values.apiKey} onChange={event => onChange({apiKey: event.target.value})}
          autoComplete="off" spellCheck={false} placeholder="Enter your provider key" />
        <p className="text-eyebrow text-text-dim">The key is saved securely and its value stays hidden.</p>
      </div>
    </div>
  );
}
