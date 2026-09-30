"use client";

import { useMemo, useRef, type RefObject } from "react";
import {
  isMessageNotSentError,
  type AppendMessage,
  type AssistantRuntime,
  type ExternalThreadQueueAdapter,
} from "@assistant-ui/react";

// The fleet's daemon is the queue: its admission ledger orders every steer, so
// no lane here ever holds a message. Once a runtime has a queue, assistant-ui
// sends every new message through it — to `steer` while a reply runs, to
// `enqueue` when idle — and never through `onNew`. That is what lets the
// thread report a run truthfully and keep Send open through it.
const NO_ITEMS: ExternalThreadQueueAdapter["items"] = [];
// The lanes are always empty, so no queue item exists for these to act on.
const NO_ITEM = (): void => undefined;
// How a returned draft joins what was typed after it (assistant-ui's own join).
const DRAFT_JOIN = "\n";

type Deliver = (message: AppendMessage) => Promise<void>;

/**
 * The queue adapter over `deliver` (the delivery path's `onNew`). A refused
 * send used to come back through `onNew`'s rejection, which assistant-ui
 * answers by returning the draft; the queue returns before the send does, so
 * the library's own rule is applied here: when no newer send has started, the
 * refused text comes back ahead of anything typed since.
 */
export function useFleetSteerQueue(
  deliver: Deliver,
  runtime: RefObject<AssistantRuntime | null>,
): ExternalThreadQueueAdapter {
  const sends = useRef(0);
  return useMemo(() => {
    const dispatch = (message: AppendMessage): void => {
      const send = ++sends.current;
      void deliver(message).catch((error: unknown) => {
        if (!isMessageNotSentError(error)) throw error;
        const composer = runtime.current?.thread.composer;
        if (composer === undefined || sends.current !== send) return;
        composer.setText([textOf(message), composer.getState().text].filter(Boolean).join(DRAFT_JOIN));
      });
    };
    return {
      items: NO_ITEMS,
      steerItems: NO_ITEMS,
      enqueue: dispatch,
      steer: dispatch,
      move: NO_ITEM,
      edit: NO_ITEM,
      remove: NO_ITEM,
    };
  }, [deliver, runtime]);
}

function textOf(message: AppendMessage): string {
  for (const part of message.content) {
    if (part.type === "text") return part.text;
  }
  return "";
}
