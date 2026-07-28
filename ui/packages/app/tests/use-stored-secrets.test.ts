import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

const listSecretsActionMock = vi.hoisted(() => vi.fn());
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  listSecretsAction: listSecretsActionMock,
}));

import { SECRETS_LOAD } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load";
import { useStoredSecrets } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/use-stored-secrets";

const secret = (name: string) => ({ name, provider: "anthropic", created_at: 1_777_507_200_000 });

/** A promise plus the handles to settle it later — the only way to hold two
 *  reads in flight at once and choose which one answers first. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => cleanup());

describe("useStoredSecrets", () => {
  it("starts idle and requests nothing — an ordinary Models visit pays for no secret list", () => {
    const { result } = renderHook(() => useStoredSecrets("ws_1"));
    expect(result.current.secretsLoad).toBe(SECRETS_LOAD.idle);
    expect(result.current.secrets).toEqual([]);
    expect(listSecretsActionMock).not.toHaveBeenCalled();
  });

  it("a loaded list is ready and readable", async () => {
    listSecretsActionMock.mockResolvedValue({ ok: true, data: { secrets: [secret("anthropic")] } });
    const { result } = renderHook(() => useStoredSecrets("ws_1"));

    await act(async () => result.current.refreshSecrets());

    await waitFor(() => expect(result.current.secretsLoad).toBe(SECRETS_LOAD.ready));
    expect(result.current.secrets.map((s) => s.name)).toEqual(["anthropic"]);
  });

  it("a slow earlier read cannot overwrite a newer one that already answered", async () => {
    // Open, close, reopen inside one gesture and two reads are in flight at
    // once. This list decides rotate-vs-create, so a stale response landing
    // last would hand the dialog a list that is no longer true — and it would
    // still call itself `ready`.
    const first = deferred<unknown>();
    const second = deferred<unknown>();
    listSecretsActionMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);

    const { result } = renderHook(() => useStoredSecrets("ws_1"));
    await act(async () => result.current.refreshSecrets());
    await act(async () => result.current.refreshSecrets());

    await act(async () => {
      second.resolve({ ok: true, data: { secrets: [secret("newest")] } });
    });
    await waitFor(() => expect(result.current.secretsLoad).toBe(SECRETS_LOAD.ready));

    // The superseded read answers last, with a list nobody asked for any more.
    await act(async () => {
      first.resolve({ ok: true, data: { secrets: [secret("stale")] } });
    });

    expect(result.current.secrets.map((s) => s.name)).toEqual(["newest"]);
    expect(result.current.secretsLoad).toBe(SECRETS_LOAD.ready);
  });

  it("a superseded read that REJECTS cannot fail a list that already arrived", async () => {
    // Same race, opposite settle: the abandoned request drops its connection
    // after the newer one succeeded. Without the generation check inside the
    // catch, the dialog would block on an error belonging to a read whose
    // answer it had already replaced.
    const first = deferred<unknown>();
    const second = deferred<unknown>();
    listSecretsActionMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);

    const { result } = renderHook(() => useStoredSecrets("ws_1"));
    await act(async () => result.current.refreshSecrets());
    await act(async () => result.current.refreshSecrets());

    await act(async () => {
      second.resolve({ ok: true, data: { secrets: [secret("newest")] } });
    });
    await waitFor(() => expect(result.current.secretsLoad).toBe(SECRETS_LOAD.ready));

    await act(async () => {
      first.reject(new Error("connection dropped"));
    });

    expect(result.current.secretsLoad).toBe(SECRETS_LOAD.ready);
    expect(result.current.secrets.map((s) => s.name)).toEqual(["newest"]);
  });

  it("a rejected REFRESH keeps the last good list live instead of locking the form", async () => {
    // `ready` is sticky. The form still holds usable data, so a failed refresh
    // must not disable Save — only a list that never loaded blocks.
    listSecretsActionMock.mockResolvedValueOnce({ ok: true, data: { secrets: [secret("anthropic")] } });
    const { result } = renderHook(() => useStoredSecrets("ws_1"));
    await act(async () => result.current.refreshSecrets());
    await waitFor(() => expect(result.current.secretsLoad).toBe(SECRETS_LOAD.ready));

    listSecretsActionMock.mockRejectedValueOnce(new Error("network down"));
    await act(async () => result.current.refreshSecrets());

    await waitFor(() => expect(listSecretsActionMock).toHaveBeenCalledTimes(2));
    expect(result.current.secretsLoad).toBe(SECRETS_LOAD.ready);
    expect(result.current.secrets.map((s) => s.name)).toEqual(["anthropic"]);
  });

  it("a rejected FIRST read lands on error, so the dialog blocks and offers retry", async () => {
    // The distinction the sticky rule turns on: nothing usable was ever
    // loaded, so failing closed is the only safe answer — the secrets POST
    // upserts, and submitting against a list that never arrived would
    // overwrite whatever already holds the typed name.
    listSecretsActionMock.mockRejectedValue(new Error("network down"));
    const { result } = renderHook(() => useStoredSecrets("ws_1"));

    await act(async () => result.current.refreshSecrets());

    await waitFor(() => expect(result.current.secretsLoad).toBe(SECRETS_LOAD.error));
    expect(result.current.secrets).toEqual([]);
  });

  it("a read that answers `ok: false` fails closed the same way a rejection does", async () => {
    listSecretsActionMock.mockResolvedValue({ ok: false, error: "Service Unavailable", status: 503 });
    const { result } = renderHook(() => useStoredSecrets("ws_1"));

    await act(async () => result.current.refreshSecrets());

    await waitFor(() => expect(result.current.secretsLoad).toBe(SECRETS_LOAD.error));
    expect(result.current.secrets).toEqual([]);
  });
});
