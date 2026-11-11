import type { Secret } from "@/lib/api/secrets";
import { SECRET_KIND } from "@/lib/api/secrets-types";
import { OPENAI_COMPATIBLE_PROVIDER } from "@/lib/types";

export const KEY_MODE = { new: "new", saved: "saved" } as const;
export type KeyMode = typeof KEY_MODE[keyof typeof KEY_MODE];
export type ModelCreationRequest = {
  workspaceId: string;
  secretRef: string;
  modelId: string;
  activate: boolean;
};
export type CreationStage = "editing" | "storage_unconfirmed" | "registration" | "registration_unconfirmed" | "activation" | "activation_unconfirmed";
export type ModelCreationState = ({stage: "editing"} | {
  stage: Exclude<CreationStage, "editing">;
  request: ModelCreationRequest;
}) & {error: string | null};
export const EMPTY_CREATION: ModelCreationState = {stage: "editing", error: null};
export type ModelFormValues = {
  provider: string; model: string; keyName: string; baseUrl: string; apiKey: string;
  mode: KeyMode; savedRef: string; activate: boolean;
};

export function compatibleCredentials(secrets: readonly Secret[], provider: string): Secret[] {
  const kind = provider === OPENAI_COMPATIBLE_PROVIDER ? SECRET_KIND.custom_endpoint : SECRET_KIND.provider_key;
  return secrets.filter(secret => secret.kind === kind && "provider" in secret && secret.provider === provider);
}

export function creationActionLabel(stage: CreationStage): string {
  switch (stage) {
    case "storage_unconfirmed": return "Choose credential";
    case "registration_unconfirmed":
    case "activation_unconfirmed": return "Check status";
    case "registration": return "Retry registration";
    case "activation": return "Retry activation";
    default: return "Create model";
  }
}
