import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listModelEntriesActionMock, listSecretsActionMock, createModelEntryActionMock, replaceSecretActionMock, createSecretActionMock, entry, registry, renderTable } from "./helpers/models-registry-fixtures";

const MODEL_ID = "claude-sonnet-5-5";

describe("Models registry stored secrets", () => {
  it("creating a model entry refreshes the secrets list — a repeat add on the same key name replaces instead of re-creating", async () => {
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: "1970-01-01T00:00:00.001Z" } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    // The dialog now loads the stored-secret list on OPEN as well as after a
    // secret changes, so the sequence has to mirror reality: nothing stored
    // when the dialog first opens, "anthropic" present from the create onward.
    // A single mockResolvedValueOnce left later calls resolving `undefined`,
    // which the real action never does.
    listSecretsActionMock
      .mockResolvedValueOnce({ ok: true, data: { secrets: [] } })
      .mockResolvedValue({
        ok: true,
        data: { secrets: [{ kind: "provider_key", name: "anthropic", provider: "anthropic", created_at: "1970-01-01T00:00:00.001Z" }] },
      });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^model$/i), MODEL_ID);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(listSecretsActionMock).toHaveBeenCalledWith("ws_1"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Second add reusing the same name ("anthropic") — the refreshed secrets
    // state now carries that name, so the dialog replaces the stored body in
    // place instead of re-creating it. This is the observable proof the
    // refreshSecrets round-trip landed in state.
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const reopened = await screen.findByRole("dialog");
    await user.type(within(reopened).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(reopened).getByLabelText(/^provider$/i), "anthropic");
    await user.type(within(reopened).getByLabelText(/^model$/i), MODEL_ID);
    await user.type(within(reopened).getByLabelText(/^api key$/i), "sk-ant-second-key");
    await user.click(within(reopened).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic", { provider: "anthropic", api_key: "sk-ant-second-key" }));
    expect(createSecretActionMock).toHaveBeenCalledTimes(1);
  });

  it("a failed refresh after a good load keeps the stored-key state live — replace still resolves", async () => {
    // `ready` is sticky: once a real list has arrived, a failed background
    // refresh must neither blank it (locking a form that has usable data) nor
    // lose the replace-vs-create resolution it feeds. Only a list that NEVER
    // loaded fails closed — that case is the test below.
    replaceSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: "1970-01-01T00:00:00.001Z" } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock
      .mockResolvedValueOnce({
        ok: true,
        data: { secrets: [{ kind: "provider_key", name: "anthropic", provider: "anthropic", created_at: "1970-01-01T00:00:00.001Z" }] },
      })
      .mockResolvedValue({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^model$/i), MODEL_ID);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Reopen: the on-open refresh now FAILS, but the earlier list is retained —
    // no fail-closed alert, and the same name still resolves to replace.
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const reopened = await screen.findByRole("dialog");
    expect(within(reopened).queryByText(/couldn't load your stored secrets/i)).toBeNull();
    await user.type(within(reopened).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(reopened).getByLabelText(/^provider$/i), "anthropic");
    await user.type(within(reopened).getByLabelText(/^model$/i), MODEL_ID);
    await user.type(within(reopened).getByLabelText(/^api key$/i), "sk-ant-second-key");
    await user.click(within(reopened).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(replaceSecretActionMock).toHaveBeenCalledWith("ws_1", "anthropic", { provider: "anthropic", api_key: "sk-ant-second-key" }));
    expect(createSecretActionMock).not.toHaveBeenCalled();
  });

  it("a failed first secret-list load fails closed — Save stays disabled until a retry lands", async () => {
    // The dialog's rotate-vs-create decision and its name-ownership guard both
    // read the stored-secret list, and the secrets POST upserts server-side.
    // Submitting against a list that never arrived would therefore stomp
    // whatever already holds the typed name — so an unloaded list must BLOCK,
    // not silently take the create path.
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: "1970-01-01T00:00:00.001Z" } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock
      .mockResolvedValueOnce({ ok: false, error: "boom", errorCode: "UZ-INTERNAL-003" })
      .mockResolvedValue({ ok: true, data: { secrets: [] } });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^model$/i), MODEL_ID);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");

    // Complete form, unknown secret list: both Save buttons are inert and the
    // dialog says why.
    await screen.findByText(/couldn't load your stored secrets/i);
    expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true);
    expect(createSecretActionMock).not.toHaveBeenCalled();

    // Retry loads the list; with it present, the same submit goes through.
    await user.click(within(dialog).getByRole("button", { name: /retry/i }));
    await waitFor(() =>
      expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(false),
    );
    await user.click(within(dialog).getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(createSecretActionMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(replaceSecretActionMock).not.toHaveBeenCalled();
  });

  it("fails closed when the secrets round-trip rejects, not just when it returns an error", async () => {
    // A rejected action is a different failure from `{ ok: false }`: the call
    // never came back at all (network drop, deploy skew). Without the catch the
    // dialog strands at "Checking your stored secrets…" — Save disabled forever
    // and no Retry, because Retry only renders in the error state.
    createSecretActionMock.mockResolvedValue({ ok: true, data: { name: "anthropic" } });
    createModelEntryActionMock.mockResolvedValue({ ok: true, data: { id: "e1", model_id: "claude-sonnet-5", secret_ref: "anthropic", created_at: "1970-01-01T00:00:00.001Z" } });
    listModelEntriesActionMock.mockResolvedValue({ ok: true, data: registry([entry({ id: "e1", secret_ref: "anthropic" })]) });
    listSecretsActionMock
      .mockRejectedValueOnce(new Error("network down"))
      .mockResolvedValue({ ok: true, data: { secrets: [] } });
    await renderTable(registry([]));

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /create model/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^provider$/i), "anthropic");
    await user.type(within(dialog).getByLabelText(/^model$/i), MODEL_ID);
    await user.type(within(dialog).getByLabelText(/^api key$/i), "sk-ant-e2e-xxxx");

    // Complete form, rejected list: same fail-closed surface as `ok: false`.
    await screen.findByText(/couldn't load your stored secrets/i);
    expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(true);
    expect(createSecretActionMock).not.toHaveBeenCalled();

    // And the same Retry recovers it.
    await user.click(within(dialog).getByRole("button", { name: /retry/i }));
    await waitFor(() =>
      expect((within(dialog).getByRole("button", { name: /^save$/i }) as HTMLButtonElement).disabled).toBe(false),
    );
  });
});
