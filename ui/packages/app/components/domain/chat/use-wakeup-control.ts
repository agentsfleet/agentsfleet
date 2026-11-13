"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { setupErrorMessage } from "@/lib/api/draft-request";

/** Mount beneath the authenticated fleet key; disposal cancels reads and writes. */
export function useWakeupControl() {
  const router = useRouter();
  const owner = useRef<AbortController | null>(null);
  const busy = useRef(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    owner.current = controller;
    return () => controller.abort();
  }, []);
  async function run<T>(action: (signal: AbortSignal) => Promise<T>, accept: (value: T) => void) {
    const signal = owner.current?.signal;
    if (!signal || signal.aborted || busy.current) return;
    const current = () => !signal.aborted;
    busy.current = true; setPending(true); setError(null);
    try {
      const result = await action(signal);
      if (current()) accept(result);
    } catch (cause) {
      if (current()) setError(setupErrorMessage(cause));
    } finally {
      if (current()) {busy.current = false; setPending(false);}
    }
  }
  return {pending, error, run, refresh: () => {setError(null); router.refresh();}};
}
