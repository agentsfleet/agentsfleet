import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Input } from "@agentsfleet/design-system";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetModelSelect } from "./FleetModelSelect";
import { EditSetupButton, FleetSetupWorkspace } from "./FleetSetupWorkspace";
import { FleetModelFixture, MODEL_OWNER, RUNTIME_MODEL } from "@/tests/helpers/fleet-model-fixture";
import { MODEL, MODEL_REF, response } from "@/tests/helpers/draft-fixtures";

const {refresh} = vi.hoisted(() => ({refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({userId: "creator", isLoaded: true})}));
const fixture = new FleetModelFixture();
beforeEach(() => {fixture.mount(); refresh.mockReset();});
afterEach(() => {cleanup(); fixture.dispose();});

function workspace(revision = "1", model = MODEL_REF, inlineEdit = false) {
  return <FleetSetupWorkspace {...MODEL_OWNER} fleet={MODEL_OWNER.fleet} revision={revision} model={model}
    viewer="creator" header={<EditSetupButton workspace={MODEL_OWNER.workspace} fleet={MODEL_OWNER.fleet} revision={revision} />}>
    <Input aria-label="Unsent fleet message" defaultValue="" />
    <FleetModelSelect label={model === MODEL_REF ? MODEL.label : RUNTIME_MODEL.label} />
    {inlineEdit ? <EditSetupButton inline /> : null}
  </FleetSetupWorkspace>;
}

async function chooseRuntime() {
  const user = userEvent.setup();
  await user.click(screen.getByRole("combobox", {name: "Fleet model"}));
  await user.click(await screen.findByRole("option", {name: RUNTIME_MODEL.label}));
}

it("preserves unsent text while the saved model changes", async () => {
  const view = render(workspace());
  const input = screen.getByRole("textbox", {name: "Unsent fleet message"});
  fireEvent.change(input, {target: {value: "Keep this unsent message"}});
  await chooseRuntime();
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  expect(screen.getByRole("combobox").textContent).toContain(RUNTIME_MODEL.label);
  view.rerender(workspace("2", RUNTIME_MODEL.ref));
  expect(screen.getByRole("textbox")).toBe(input);
  expect(input).toHaveProperty("value", "Keep this unsent message");
  expect(screen.getByRole("combobox").textContent).toContain(RUNTIME_MODEL.label);
  expect(fixture.calls.some(call => call.path.endsWith("/messages") && call.method === "POST")).toBe(false);
});

it("keeps the confirmed model if route refresh becomes unavailable after saving", async () => {
  refresh.mockImplementationOnce(() => {throw new Error("Route refresh unavailable");});
  render(workspace());
  await chooseRuntime();
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  expect(screen.getByRole("combobox").textContent).toContain(RUNTIME_MODEL.label);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fixture.applied.size).toBe(1);
});

it("keeps the saved label during an unconfirmed apply and offers its retained retry", async () => {
  fixture.loseApply = true;
  render(workspace("1", MODEL_REF, true));
  expect(screen.getAllByRole("link", {name: "Edit setup"})).toHaveLength(2);
  await chooseRuntime();
  await screen.findByRole("button", {name: "Retry"});
  expect(refresh).not.toHaveBeenCalled();
  expect(screen.getByRole("combobox")).toHaveProperty("disabled", true);
  expect(screen.getByRole("combobox").textContent).toContain(MODEL.label);
  const edits = screen.getAllByRole("button", {name: "Edit setup"});
  expect(edits).toHaveLength(2);
  for (const edit of edits) expect(edit).toHaveProperty("disabled", true);
  await userEvent.click(screen.getByRole("button", {name: "Retry"}));
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  expect(screen.getByRole("combobox").textContent).toContain(RUNTIME_MODEL.label);
  expect(fixture.applyKeys).toHaveLength(2);
  expect(fixture.applyKeys[1]).toBe(fixture.applyKeys[0]);
  expect(screen.getAllByRole("link", {name: "Edit setup"})).toHaveLength(2);
});

it.each(["source", "apply"])("reloads a stale fleet before saving another model (%s)", async (step) => {
  if (step === "source") fixture.revision = "2"; else fixture.saveStatus = 412;
  const view = render(workspace());
  const input = screen.getByRole("textbox", {name: "Unsent fleet message"});
  fireEvent.change(input, {target: {value: "Keep this unsent message"}});
  await chooseRuntime();
  expect((await screen.findByRole("alert")).textContent).toContain(step === "source" ? "fleet changed" : "Another editor saved a newer version.");
  expect(screen.getByRole("combobox").textContent).toContain(MODEL.label);
  expect(screen.getByRole("combobox")).toHaveProperty("disabled", true);
  expect(refresh).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", {name: "Reload fleet"}));
  expect(refresh).toHaveBeenCalledOnce();
  fixture.revision = "2"; fixture.saveStatus = 201;
  view.rerender(workspace("2"));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(screen.getByRole("textbox")).toBe(input);
  expect(input).toHaveProperty("value", "Keep this unsent message");
  await chooseRuntime();
  await waitFor(() => expect(refresh).toHaveBeenCalledTimes(2));
  expect(screen.getByRole("combobox").textContent).toContain(RUNTIME_MODEL.label);
});

it("reports empty runtime choices and recovers a failed model-list read", async () => {
  const normal = fixture.override;
  fixture.override = call => call.path.endsWith("/models") ? response({}, 503) : normal(call);
  render(workspace());
  await userEvent.click(screen.getByRole("combobox"));
  expect((await screen.findByRole("option", {name: "Models couldn’t be loaded. Try again."})).getAttribute("aria-disabled")).toBe("true");
  fireEvent.keyDown(screen.getByRole("listbox"), {key: "Escape"});
  expect((await screen.findByRole("alert")).textContent).toContain("Models couldn’t be loaded");
  fixture.override = normal;
  await userEvent.click(screen.getByRole("button", {name: "Reload choices"}));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(fixture.mutations()).toEqual([]);
});

it("keeps a stale fleet locked when route refresh is unavailable", async () => {
  fixture.saveStatus = 412;
  refresh.mockImplementationOnce(() => {throw new Error("Route refresh unavailable");});
  render(workspace());
  await chooseRuntime();
  await userEvent.click(await screen.findByRole("button", {name: "Reload fleet"}));
  expect(refresh).toHaveBeenCalledOnce();
  expect(screen.getByRole("combobox")).toHaveProperty("disabled", true);
  expect(screen.getByRole("combobox").textContent).toContain(MODEL.label);
  expect(screen.getByRole("alert").textContent).toContain("Another editor saved a newer version.");
  expect(fixture.applied.size).toBe(0);
});

it("recovers a refused review when refreshing keeps the same fleet revision", async () => {
  fixture.saveStatus = 409;
  const view = render(workspace());
  const input = screen.getByRole("textbox", {name: "Unsent fleet message"});
  fireEvent.change(input, {target: {value: "Keep my unsent message"}});
  await chooseRuntime();
  await screen.findByRole("alert");
  fixture.saveStatus = 201;
  await userEvent.click(screen.getByRole("button", {name: "Reload fleet"}));
  expect(refresh).toHaveBeenCalledOnce();
  view.rerender(workspace());
  expect(screen.getByRole("combobox")).toHaveProperty("disabled", false);
  expect(screen.getByRole("textbox")).toBe(input);
  expect(input).toHaveProperty("value", "Keep my unsent message");
  await chooseRuntime();
  await waitFor(() => expect(refresh).toHaveBeenCalledTimes(2));
  expect(screen.getByRole("combobox").textContent).toContain(RUNTIME_MODEL.label);
  expect(fixture.applied.size).toBe(1);
});

it("renders the saved label without workspace editing context", () => {
  render(<FleetModelSelect label="Saved runtime model" />);
  expect(screen.getByText("Saved runtime model")).toBeTruthy();
  expect(screen.queryByRole("combobox")).toBeNull();
  expect(fixture.calls).toEqual([]);
});

it("loads more runtime choices and leaves the list open without another initial read", async () => {
  let reads = 0;
  fixture.override = call => call.path.endsWith("/models") ? response({
    items: reads++ ? [RUNTIME_MODEL] : [MODEL], next_cursor: reads === 1 ? "next" : null,
  }) : undefined;
  render(workspace());
  await userEvent.click(screen.getByRole("combobox"));
  await screen.findByRole("option", {name: MODEL.label});
  fireEvent.keyDown(screen.getByRole("listbox"), {key: "Escape"});
  await userEvent.click(screen.getByRole("button", {name: "Load more models"}));
  await waitFor(() => expect(screen.queryByRole("button", {name: "Load more models"})).toBeNull());
  await userEvent.click(screen.getByRole("combobox"));
  expect(await screen.findByRole("option", {name: RUNTIME_MODEL.label})).toBeTruthy();
  expect(reads).toBe(2);
});

it("removes reload actions when workspace access is lost", async () => {
  fixture.override = () => response({}, 403);
  render(workspace());
  await userEvent.click(screen.getByRole("combobox"));
  fireEvent.keyDown(await screen.findByRole("listbox"), {key: "Escape"});
  await screen.findByRole("alert");
  expect(screen.queryByRole("button", {name: "Reload choices"})).toBeNull();
  expect(screen.getByRole("combobox")).toHaveProperty("disabled", true);
});
