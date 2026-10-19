import { describe, expect, it } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { listModelEntriesActionMock, entry, registry, renderTablePaged, findEnabledRetry } from "./helpers/models-registry-fixtures";

describe("Models registry paging and failures", () => {
  it("Load more appends the next page, retains prior rows, and discloses the unnamed remainder", async () => {
    // The registry's own Invariant-5 surface, mirroring the gallery suite: the
    // walk this replaced guaranteed every row was present; paging must retain
    // what is loaded and say what is not.
    listModelEntriesActionMock.mockResolvedValue({
      ok: true,
      data: { ...registry([entry({ id: "e2", model_id: "gpt-5", secret_ref: "openai" })]), next_cursor: null, total: null },
    });
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    // total is null on every daemon endpoint today, so this wording is the
    // one production users actually see.
    expect(screen.getByText("Showing 1 models — more available")).toBeTruthy();

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Next page" }));

    expect(listModelEntriesActionMock).toHaveBeenCalledWith("cur-2");
    expect(await screen.findByTitle("gpt-5")).toBeTruthy();
    // Prior rows RETAINED, not replaced.
    expect(screen.getByTitle("claude-sonnet-5")).toBeTruthy();
    // Last page reached — the affordance and its disclosure both retire.
    expect(screen.getByRole("button", { name: "Next page" })).toHaveProperty("disabled", true);
  });

  it("a failed load-more keeps the rows on screen and offers retry — never an empty registry", async () => {
    listModelEntriesActionMock.mockResolvedValue({ ok: false, error: "upstream 503", status: 503 });
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Next page" }));

    expect(screen.getByTitle("claude-sonnet-5")).toBeTruthy();
    // The preserved 503 keeps its specific instruction.
    expect(await screen.findByText("Models are temporarily unavailable. Your entries are safe.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("a failed server read renders the typed failure on the real table, distinct from empty", async () => {
    await renderTablePaged({
      initialPage: null,
      initialError: { kind: "unavailable" },
    });
    expect(screen.getByText("Models are temporarily unavailable. Your entries are safe.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("treats a cursor that does not advance as the last page instead of appending forever", async () => {
    listModelEntriesActionMock.mockResolvedValue({
      ok: true,
      data: { ...registry([entry({ id: "e2", model_id: "gpt-5", secret_ref: "openai" })]), next_cursor: "cur-2", total: null },
    });
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(await screen.findByTitle("gpt-5")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Next page" })).toHaveProperty("disabled", true);
  });

  it("a rejected action round-trip surfaces as a read failure instead of escaping the transition", async () => {
    listModelEntriesActionMock.mockRejectedValue(new Error("network down"));
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(await screen.findByText("Could not load your models. They have not been changed.")).toBeTruthy();
    expect(screen.getByTitle("claude-sonnet-5")).toBeTruthy();
  });

  it("a thrown non-Error still reports the failure, with no fabricated detail", async () => {
    // `detail` is the thrown message, and only an Error carries one. A rejected
    // string (or anything else) must still surface the failure rather than
    // render `undefined` at the operator as if it were a reason.
    listModelEntriesActionMock.mockRejectedValue("bare string, not an Error");
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(await screen.findByText("Could not load your models. They have not been changed.")).toBeTruthy();
    expect(screen.queryByText(/bare string, not an Error/)).toBeNull();
  });

  it("a rejected Retry re-reports the failure instead of escaping the transition", async () => {
    // Retry re-reads from the FIRST page, which is a different request from
    // load-more and has its own catch. An uncaught rejection here escapes the
    // transition into a route with no error boundary, and the operator is left
    // clicking a Retry that silently does nothing.
    listModelEntriesActionMock.mockRejectedValue(new Error("still down"));
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(await findEnabledRetry());

    // Retry re-reads from the first page, so it sends NO cursor — that is what
    // makes it a different request from the load-more that failed.
    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalledTimes(2));
    expect(listModelEntriesActionMock).toHaveBeenLastCalledWith();
    expect(await screen.findByText("Could not load your models. They have not been changed.")).toBeTruthy();
    // Retained rows survive a second failure — a failed re-read never blanks
    // the table it was meant to refresh.
    expect(screen.getByTitle("claude-sonnet-5")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("a Retry that rejects with a non-Error reports the failure without a fabricated detail", async () => {
    listModelEntriesActionMock.mockRejectedValue("bare string, not an Error");
    await renderTablePaged({
      initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: "cur-2", total: null },
    });

    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(await findEnabledRetry());

    await waitFor(() => expect(listModelEntriesActionMock).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("Could not load your models. They have not been changed.")).toBeTruthy();
    expect(screen.queryByText(/bare string, not an Error/)).toBeNull();
  });

  it("readErrorFrom maps a preserved transport status onto the typed vocabulary", async () => {
    const { readErrorFrom } = await import("@/lib/api/library-types");
    expect(readErrorFrom({ error: "no session", status: 401 }).kind).toBe("unauthenticated");
    expect(readErrorFrom({ error: "nope", status: 403 }).kind).toBe("forbidden");
    expect(readErrorFrom({ error: "down", status: 503 }).kind).toBe("unavailable");
    // No status (thrown before transport, or a non-Api failure) → unknown,
    // never a guessed specific instruction.
    expect(readErrorFrom({ error: "boom" }).kind).toBe("unknown");
    expect(readErrorFrom({ error: "boom" }).detail).toBe("boom");
  });

  it("libraryErrorFromCause reads a status off the cause and a detail only off an Error", async () => {
    const { libraryErrorFromCause } = await import("@/lib/api/library-types");
    // An ApiError-shaped throw keeps its specific kind.
    expect(libraryErrorFromCause({ status: 503 }).kind).toBe("unavailable");
    // A thrown Error contributes its message as the detail...
    expect(libraryErrorFromCause(new Error("threw")).detail).toBe("threw");
    expect(libraryErrorFromCause(new Error("threw")).kind).toBe("unknown");
    // ...and a thrown non-Error has no message to contribute, so it must not
    // invent one rather than render `undefined` at the operator as a reason.
    expect(libraryErrorFromCause("bare string").detail).toBeUndefined();
  });

  it("readErrorCopy gives each failure kind its own next step", async () => {
    const { readErrorCopy } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view"
    );
    const { LIBRARY_ERROR_KIND } = await import("@/lib/api/library-types");
    // Five kinds, five distinct instructions — collapsing any two loses the
    // reason these are typed at all.
    const copies = Object.values(LIBRARY_ERROR_KIND).map((kind) => readErrorCopy({ kind }));
    expect(new Set(copies).size).toBe(copies.length);
    expect(readErrorCopy({ kind: LIBRARY_ERROR_KIND.unauthenticated })).toMatch(/sign in/i);
    expect(readErrorCopy({ kind: LIBRARY_ERROR_KIND.unavailable })).toMatch(/temporarily unavailable/i);
  });

  it("computeNextSort ignores a key outside the sortable column set, and toggles both directions", async () => {
    const { computeNextSort } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view"
    );
    expect(computeNextSort(null, "status")).toBeNull();
    expect(computeNextSort({ key: "model", dir: "ascending" }, "actions")).toBeNull();
    expect(computeNextSort({ key: "model", dir: "ascending" }, "model")).toEqual({ key: "model", dir: "descending" });
    expect(computeNextSort({ key: "model", dir: "descending" }, "model")).toEqual({ key: "model", dir: "ascending" });
  });

});
