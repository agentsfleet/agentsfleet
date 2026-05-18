import { useEffect, useMemo, useRef } from "react";

// Imperative one-shot timer with cancel-and-reschedule + auto-cleanup
// on unmount. Each `start(cb, ms)` cancels the prior pending callback
// before scheduling, so call sites don't need their own useRef +
// useEffect cleanup pattern. The callback closure is fresh per call,
// so consumers can read the latest state (e.g. setCopiedKey((k) => k
// === key ? null : k) where `key` is the current key, not the key at
// hook creation).
export type ResettableTimeout = {
  start: (cb: () => void, ms: number) => void;
  cancel: () => void;
};

export function useResettableTimeout(): ResettableTimeout {
  const handle = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (handle.current !== null) {
        clearTimeout(handle.current);
        handle.current = null;
      }
    },
    [],
  );

  return useMemo(
    () => ({
      start(cb: () => void, ms: number) {
        if (handle.current !== null) clearTimeout(handle.current);
        handle.current = setTimeout(() => {
          handle.current = null;
          cb();
        }, ms);
      },
      cancel() {
        if (handle.current !== null) {
          clearTimeout(handle.current);
          handle.current = null;
        }
      },
    }),
    [],
  );
}
