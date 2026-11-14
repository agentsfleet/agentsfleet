import type { useStoredSecrets } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/use-stored-secrets";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { SetupSelectionEditor } from "./SetupSelectionEditor";
import { SetupUnavailable } from "@/lib/api/draft-request";

const {catalogue, user} = vi.hoisted(() => ({catalogue: vi.fn<typeof useStoredSecrets>(), user: {userId: "creator"}}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/use-stored-secrets", () => ({useStoredSecrets: catalogue}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => user}));
const reload = vi.fn();
const actions = {disabled: false, pending: false, onRetry: vi.fn(), onReload: vi.fn(), onApply: vi.fn(), onCancel: vi.fn()};
const props = {workspace: "workspace", selected: ["github"], kind: "credentials" as const, ...actions};
beforeEach(() => {vi.clearAllMocks();catalogue.mockReturnValue({secrets: [{name: "slack", kind: "custom_secret", created_at: "0"}], secretsLoad: "ready", refreshSecrets: reload});});
afterEach(cleanup);

// Value: protects=secret-name-only selection and explicit catalog readiness;
// fails_when=unavailable secrets look empty or opening reads plaintext; seam=existing scoped catalog hook; no_claim=vault storage.
it("loads names on intent and applies only selected names", async () => {
  render(<SetupSelectionEditor {...props} />);
  expect(reload).toHaveBeenCalledOnce();
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(screen.getByRole("checkbox", {name: "github"}));
  fireEvent.click(screen.getByRole("checkbox", {name: "slack"}));
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  await waitFor(() => expect(actions.onApply).toHaveBeenCalledExactlyOnceWith(["slack"]));
});

it("keeps unavailable and loading catalogs distinct from confirmed empty lists", () => {
  catalogue.mockReturnValue({secrets: [], secretsLoad: "error", refreshSecrets: reload});
  const {rerender} = render(<SetupSelectionEditor {...props} selected={[]} />);
  fireEvent.click(screen.getByRole("button", {name: "Retry secrets"}));
  expect(reload).toHaveBeenCalledTimes(2);
  expect(screen.getByRole("button", {name: "Apply selection"})).toHaveProperty("disabled", true);
  catalogue.mockReturnValue({secrets: [], secretsLoad: "loading", refreshSecrets: reload});rerender(<SetupSelectionEditor {...props} selected={[]} />);
  expect(screen.getByText("Loading secret names")).toBeTruthy();
  catalogue.mockReturnValue({secrets: [], secretsLoad: "ready", refreshSecrets: reload});rerender(<SetupSelectionEditor {...props} selected={[]} />);
  expect(screen.getByText("No secrets are available in this workspace.")).toBeTruthy();
});

it.each(["credentials", "repositories"] as const)("retains manual %s selections after a concurrent change", kind => {
  const {rerender} = render(<SetupSelectionEditor {...props} kind={kind} />);
  if (kind === "repositories") fireEvent.change(screen.getByRole("textbox"), {target: {value: "agentsfleet/docs"}});
  rerender(<SetupSelectionEditor {...props} kind={kind} selected={["changed-elsewhere"]} />);
  expect(screen.getByRole("alert").textContent).toContain("Your edits are still here");
  expect(screen.getByRole("button", {name: "Apply selection"})).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));expect(actions.onCancel).toHaveBeenCalledOnce();
});

it("blocks duplicates and preserves the same pending update for Retry", async () => {
  const pending = Promise.withResolvers<void>();actions.onRetry.mockReturnValue(pending.promise);
  const {rerender} = render(<SetupSelectionEditor {...props} disabled pending />);
  const retry = screen.getByRole("button", {name: "Retry update"});
  fireEvent.click(retry);fireEvent.click(retry);
  expect(actions.onRetry).toHaveBeenCalledOnce();expect(actions.onApply).not.toHaveBeenCalled();
  await act(async () => {pending.reject(new SetupUnavailable("Retry still unavailable", 503));});
  expect(screen.getByRole("alert").textContent).toContain("Retry still unavailable");
  rerender(<SetupSelectionEditor {...props} disabled />);
  fireEvent.submit(screen.getByRole("button", {name: "Apply selection"}).closest("form")!);
  expect(actions.onApply).not.toHaveBeenCalled();
});

it("retains a reload failure so it can be tried again", async () => {
  actions.onApply.mockRejectedValueOnce(new Error("Selection refused"));actions.onReload.mockRejectedValueOnce(new SetupUnavailable("Reload refused", 503));
  render(<SetupSelectionEditor {...props} />);
  fireEvent.click(screen.getByRole("button", {name: "Apply selection"}));
  fireEvent.click(await screen.findByRole("button", {name: "Refresh setup"}));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Reload refused"));
});
