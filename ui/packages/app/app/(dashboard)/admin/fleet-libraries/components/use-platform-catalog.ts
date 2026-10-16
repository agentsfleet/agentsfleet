"use client";

import type { PlatformEntry } from "@/lib/api/platform-library-types";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { deletePlatformLibraryAction, patchPlatformLibraryAction } from "../actions";
import { DELETE_ACTION, PATCH_ACTION } from "../library-copy";

type Override = {base: string; entry: PlatformEntry};

export function usePlatformCatalog(entries: PlatformEntry[]) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [overrides, setOverrides] = useState<Record<string, Override>>({});
  const [deleted, setDeleted] = useState<Record<string, string>>({});
  const current = entries.filter((entry) => deleted[entry.id] !== entry.etag).map((entry) => {
    const update = overrides[entry.id];
    return update?.base === entry.etag ? update.entry : entry;
  });
  function originalTag(id: string, tag: string): string {
    const source = entries.find((entry) => entry.id === id);
    const override = overrides[id];
    return override?.base === source?.etag && override?.entry.etag === tag ? override.base : tag;
  }
  function remember(base: string, entry: PlatformEntry) {
    setOverrides((previous) => ({...previous, [entry.id]: {base: originalTag(entry.id, base), entry}}));
  }
  async function mutate(entry: PlatformEntry, published?: boolean) {
    setBusy(true); setError(null);
    try {
      const result = published === undefined
        ? await deletePlatformLibraryAction(entry.id, entry.etag)
        : await patchPlatformLibraryAction(entry.id, {published}, entry.etag);
      if (!result.ok) {
        setError(presentErrorString({errorCode: result.errorCode, message: result.error, action: published === undefined ? DELETE_ACTION : PATCH_ACTION}));
        router.refresh();
        return false;
      }
      if (result.data) remember(entry.etag, result.data);
      else setDeleted((previous) => ({...previous, [entry.id]: originalTag(entry.id, entry.etag)}));
      return true;
    } catch (cause) {
      setError(unexpectedErrorMessage(cause, published === undefined ? DELETE_ACTION : PATCH_ACTION));
      router.refresh();
      return false;
    } finally { setBusy(false); }
  }
  return {current, busy, error, clearError: () => setError(null), remember, mutate};
}
