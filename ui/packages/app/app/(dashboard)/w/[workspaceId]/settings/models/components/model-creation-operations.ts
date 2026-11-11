import type { ActionResult } from "@/lib/actions/with-token";
import { isDefiniteRefusal } from "@/lib/api/errors";
import { presentErrorString, UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";
import { createSecretAction } from "@/app/(dashboard)/w/[workspaceId]/secrets/actions";
import { createModelEntryAction, findRegisteredModelAction, readModelActivationAction, setProviderSelfManagedAction } from "../actions";
import { captureModelActivated } from "../lib/track";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EMPTY_CREATION, type ModelCreationRequest, type ModelCreationState } from "./model-creation";

export type CreationCallbacks = {
  onCreated: () => void;
  onSecretsChanged: () => void;
  clearKey: () => void;
  onComplete: () => void;
};
type Failure = Extract<ActionResult<unknown>, {ok: false}>;
type Operation = {signal: AbortSignal; callbacks: CreationCallbacks};
const cancelled = (operation: Operation) => operation.signal.aborted;
const reference = (request: ModelCreationRequest) => ({model_id: request.modelId, secret_ref: request.secretRef});
const failureText = (failure: Failure, action: string, prefix = "") => prefix + presentErrorString({errorCode: failure.errorCode, message: failure.error, action});

// The action's own HTTP round trip can fail before withToken's result arrives.
async function attempt<T>(call: () => Promise<ActionResult<T>>): Promise<ActionResult<T>> {
  try { return await call(); }
  catch { return {ok: false, error: UNCONFIRMED_REQUEST_MESSAGE}; }
}

export async function createModel(request: ModelCreationRequest, data: Record<string, unknown> | undefined, operation: Operation): Promise<ModelCreationState> {
  if (cancelled(operation)) return EMPTY_CREATION;
  if (data !== undefined) {
    const stored = await attempt(() => createSecretAction(request.workspaceId, {name: request.secretRef, data}));
    if (cancelled(operation)) return EMPTY_CREATION;
    if (!stored.ok) {
      const refused = isDefiniteRefusal(stored.status);
      if (!refused) { operation.callbacks.clearKey(); operation.callbacks.onSecretsChanged(); }
      const error = failureText(stored, "store the credential", refused ? "" : "Storage is unconfirmed. Choose a saved credential or a new free name before continuing. ");
      return refused ? {...EMPTY_CREATION, error} : {stage: "storage_unconfirmed", request, error};
    }
    operation.callbacks.clearKey();
    operation.callbacks.onSecretsChanged();
    captureProductEvent(EVENTS.secret_added, {secret_name: request.secretRef});
  }
  return registerModel(request, operation);
}

async function registerModel(request: ModelCreationRequest, operation: Operation): Promise<ModelCreationState> {
  if (cancelled(operation)) return EMPTY_CREATION;
  const result = await attempt(() => createModelEntryAction(reference(request)));
  if (cancelled(operation)) return EMPTY_CREATION;
  if (!result.ok) return {
    stage: isDefiniteRefusal(result.status) && result.status !== 409 ? "registration" : "registration_unconfirmed", request,
    error: failureText(result, "register the model entry", "Credential retained. "),
  };
  return finishRegistration(request, operation);
}

async function finishRegistration(request: ModelCreationRequest, operation: Operation): Promise<ModelCreationState> {
  if (cancelled(operation)) return EMPTY_CREATION;
  operation.callbacks.onCreated();
  if (request.activate) return activateModel(request, operation);
  operation.callbacks.onComplete();
  return EMPTY_CREATION;
}

async function activateModel(request: ModelCreationRequest, operation: Operation): Promise<ModelCreationState> {
  if (cancelled(operation)) return EMPTY_CREATION;
  const result = await attempt(() => setProviderSelfManagedAction({secret_ref: request.secretRef, model: request.modelId}));
  if (cancelled(operation)) return EMPTY_CREATION;
  if (!result.ok) return {
    stage: isDefiniteRefusal(result.status) ? "activation" : "activation_unconfirmed", request,
    error: failureText(result, "activate this model", "Model created. "),
  };
  captureModelActivated(result.data);
  operation.callbacks.onCreated();
  operation.callbacks.onComplete();
  return EMPTY_CREATION;
}

export async function retryModel(previous: ModelCreationState, operation: Operation): Promise<ModelCreationState> {
  if (cancelled(operation) || previous.stage === "editing" || previous.stage === "storage_unconfirmed") return previous;
  const {request, stage} = previous;
  if (stage === "registration") return registerModel(request, operation);
  if (stage === "activation") return activateModel(request, operation);
  if (stage === "registration_unconfirmed") {
    const result = await attempt(() => findRegisteredModelAction(reference(request)));
    if (cancelled(operation)) return EMPTY_CREATION;
    if (!result.ok) return {...previous, error: failureText(result, "check model registration")};
    return result.data ? finishRegistration(request, operation) : {
      stage: "registration", request, error: "The model was not found. Retry registration using the retained credential.",
    };
  }
  const result = await attempt(() => readModelActivationAction(reference(request)));
  if (cancelled(operation)) return EMPTY_CREATION;
  if (!result.ok) return {...previous, error: failureText(result, "check model activation")};
  operation.callbacks.onCreated();
  if (result.data) { operation.callbacks.onComplete(); return EMPTY_CREATION; }
  return {stage: "activation", request, error: "Model created. Retry activation to make it active."};
}
