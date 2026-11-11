"use client";

import { startTransition, useActionState, useEffect, useEffectEvent, useRef } from "react";
import { createModel, retryModel, type CreationCallbacks } from "./model-creation-operations";
import { EMPTY_CREATION, type ModelCreationRequest, type ModelCreationState } from "./model-creation";

type Command = {kind: "reset"} | {kind: "retry"} | {
  kind: "create"; request: ModelCreationRequest; data?: Record<string, unknown>;
};
type Input = {command: Command; signal: AbortSignal};

export function useModelCreation(callbacks: CreationCallbacks, onPendingChange: (pending: boolean) => void) {
  const busy = useRef(false);
  const active = useRef<AbortController | null>(null);
  const releasePending = useEffectEvent(() => onPendingChange(false));
  useEffect(() => () => {
    active.current?.abort();
    releasePending();
  }, []);

  const [state, dispatch, pending] = useActionState<ModelCreationState, Input>(async (previous, input) => {
    const operation = {signal: input.signal, callbacks};
    try {
      if (input.command.kind === "reset") return EMPTY_CREATION;
      if (input.command.kind === "retry") return await retryModel(previous, operation);
      if (previous.stage !== "editing") return previous;
      return await createModel(input.command.request, input.command.data, operation);
    } finally {
      if (!input.signal.aborted) { busy.current = false; onPendingChange(false); }
    }
  }, EMPTY_CREATION);

  function send(command: Command) {
    // React Actions queue dispatches. Guard the gap before pending is painted.
    if (busy.current) return;
    busy.current = true;
    const controller = new AbortController();
    active.current = controller;
    onPendingChange(true);
    startTransition(() => dispatch({command, signal: controller.signal}));
  }

  return {
    state, pending,
    create: (request: ModelCreationRequest, data?: Record<string, unknown>) => send({kind: "create", request, data}),
    retry: () => send({kind: "retry"}),
    reset: () => send({kind: "reset"}),
  };
}
