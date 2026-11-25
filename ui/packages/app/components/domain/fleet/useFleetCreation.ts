"use client";

import type { AppendMessage } from "@assistant-ui/react";
import type { PendingSend } from "../conversation/delivery/useFleetPendingSends";
import { MessageNotSentError } from "@assistant-ui/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { createFleet } from "@/lib/api/fleet-client";
import { definiteRefusal, RequestFailed, requestErrorMessage } from "@/lib/api/live-request";
import { fleetMessageText } from "@/lib/chat-message-text";
import { mintOperationId } from "@/lib/streaming/operation-id";
import { workspacePath } from "@/lib/workspace-routes";
import { PENDING_SEND_STATE } from "../conversation/delivery/useFleetPendingSends";

export type FleetCreation = {
  creating: boolean;
  /** The first message that made no fleet yet, which the composer offers to send again. */
  pending: readonly PendingSend[];
  /** Why that message made no fleet, in the daemon's words where it gave them. */
  error: string | null;
  onNew: (message: AppendMessage) => Promise<void>;
  resend: (operationId: string) => void;
  dismiss: (operationId: string) => void;
};

type CreateOperation = {operationId: string; model: string | null};
type PendingCreation = PendingSend & CreateOperation;

const NO_PENDING: readonly PendingSend[] = [];
const MINT_UNAVAILABLE = "This browser cannot name the message, so it was not sent. Try another browser.";

/**
 * Sends a first message as `POST W/fleets`, then opens the fleet it made. A
 * refused message comes back to the composer with its reason and a Resend; a
 * message whose answer never came is sent again under the same operation, so
 * the daemon answers with the fleet it may already have made.
 */
export function useFleetCreation(workspace: string, model: string | null): FleetCreation {
  const {creating, unsent, error, latest, create, remember, setError} = useCreateRequest(workspace);

  const onNew = useCallback(async (message: AppendMessage): Promise<void> => {
    const text = fleetMessageText(message);
    if (text.length === 0) return;
    const operation = operationFor(latest.current, text, model);
    if (operation === null) {setError(MINT_UNAVAILABLE); throw new MessageNotSentError();}
    if (!await create(operation, text)) throw new MessageNotSentError();
  }, [create, latest, model, setError]);

  const resend = useCallback((operationId: string): void => {
    const entry = latest.current;
    if (entry?.operationId !== operationId) return;
    const sendAs = operationFor(entry, entry.text, model);
    if (sendAs === null) {setError(MINT_UNAVAILABLE); return;}
    void create(sendAs, entry.text);
  }, [create, latest, model, setError]);

  const dismiss = useCallback((operationId: string): void => {
    if (latest.current?.operationId !== operationId) return;
    remember(null);
    setError(null);
  }, [latest, remember, setError]);

  const pending = useMemo(() => unsent ? [unsent] : NO_PENDING, [unsent]);
  return useMemo(() => ({creating, pending, error, onNew, resend, dismiss}), [creating, pending, error, onNew, resend, dismiss]);
}

/** One `POST W/fleets` at a time, cancelled with its page; a create that made no fleet stays as the unsent message. */
function useCreateRequest(workspace: string) {
  const router = useRouter();
  const [creating, setCreating] = useState(false);
  const [unsent, setUnsent] = useState<PendingCreation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef<PendingCreation | null>(null);
  const busy = useRef(false);
  const owner = useRef(new AbortController());
  useEffect(() => {
    const controller = new AbortController();
    owner.current = controller;
    return () => controller.abort();
  }, []);
  const remember = useCallback((entry: PendingCreation | null) => {latest.current = entry; setUnsent(entry);}, []);
  const create = useCallback(async ({operationId, model}: CreateOperation, text: string): Promise<boolean> => {
    if (busy.current) return false;
    busy.current = true;
    setCreating(true); setError(null);
    try {
      const created = await createFleet(workspace, {message: text, operation_id: operationId, ...(model === null ? {} : {model_ref: model})}, owner.current.signal);
      remember(null);
      router.replace(workspacePath(workspace, `fleets/${encodeURIComponent(created.fleetId)}`));
      return true;
    } catch (cause) {
      remember({operationId, model, text, submittedAtMs: Date.now(), state: outcomeOf(cause)});
      setError(requestErrorMessage(cause));
      return false;
    } finally {
      busy.current = false;
      setCreating(false);
    }
  }, [workspace, router, remember]);
  return {creating, unsent, error, latest, create, remember, setError};
}

// An unconfirmed create may already exist. Replay the model and operation the
// daemon saw so a switch in the picker keeps the original request recoverable.
function operationFor(previous: PendingCreation | null, text: string, model: string | null): CreateOperation | null {
  if (previous?.state === PENDING_SEND_STATE.UNKNOWN && previous.text === text) return previous;
  try {return {operationId: mintOperationId(), model};} catch {return null;}
}

function outcomeOf(cause: unknown): PendingSend["state"] {
  if (cause instanceof RequestFailed && cause.status === 401) return PENDING_SEND_STATE.SESSION;
  return definiteRefusal(cause) ? PENDING_SEND_STATE.REFUSED : PENDING_SEND_STATE.UNKNOWN;
}
