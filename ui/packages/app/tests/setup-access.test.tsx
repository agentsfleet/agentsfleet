import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SetupAccess } from "@/components/domain/chat/SetupAccess";
import { FleetSetup } from "@/components/domain/chat/FleetSetup";
import { FleetRecoveryContent } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetRecoveryContent";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { DRAFT, FLEET_DETAIL, FLEET_ID, REVIEW, WORKSPACE } from "./helpers/draft-fixtures";

const api = vi.hoisted(() => ({catalog: vi.fn(), connect: vi.fn(), create: vi.fn(), refresh: vi.fn()}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/integrations/connector-actions", () => ({readConnectorCatalogAction: api.catalog, startConnectAction: api.connect}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({createSecretAction: api.create}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh: api.refresh})}));
vi.mock("@/lib/analytics/posthog", () => ({captureProductEvent: vi.fn()}));
const HOST = "api.fly.io";
const FORM_VALUE = ["form", "test", "value"].join("-");
const definition = {...REVIEW.effective.definition, instructions_md: "Use ${secrets.fly.api_token}.",
  config: {...REVIEW.effective.definition.config, "x-agentsfleet": {network: {allow: [HOST]}}}};
beforeEach(() => {vi.resetAllMocks(); api.catalog.mockResolvedValue({ok: true, data: []}); api.create.mockResolvedValue({ok: true, data: {name: "fly"}});});
afterEach(() => {cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals();});

function authorizationWindow() {
  vi.stubGlobal("opener", window);
  const close = vi.spyOn(window, "close").mockImplementation(() => {});
  const navigate = vi.spyOn(window.location, "replace").mockImplementation(() => {});
  return {popup: window, close, navigate};
}

it("refreshes the fleet page after checking newly authorized setup access", async () => {
  const provider = "zoho";
  api.catalog.mockResolvedValue({ok: true, data: [{id: provider, display_name: "Zoho", configured: true}]});
  api.connect.mockResolvedValue({ok: true, data: {install_url: "https://accounts.zoho.test/authorize"}});
  const {popup} = authorizationWindow();
  vi.spyOn(window, "open").mockReturnValue(popup);
  const fleet = {...FLEET_DETAIL, setup: {state: "waiting", checks: [{kind: "credentials", subject: provider, state: "blocked"}]}};
  render(<FleetRecoveryContent workspace={WORKSPACE} fleet={fleet} setupPending />);
  fireEvent.click(screen.getByRole("button", {name: "Connect Zoho"}));
  await screen.findByText("Finish authorizing in the new tab, then check again here.");
  expect(api.refresh).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Check again"}));
  await waitFor(() => expect(api.refresh).toHaveBeenCalledOnce());
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("heading", {name: "Save done v1"})).toBeTruthy();
  expect(api.create).not.toHaveBeenCalled();
});

async function openForm(refresh = vi.fn(async () => {})) {
  render(<SetupAccess workspace={WORKSPACE} name="fly" definition={definition} onRefresh={refresh} />);
  fireEvent.click(screen.getByRole("button", {name: "Connect Fly"}));
  await screen.findByRole("dialog");
  return refresh;
}

it("stores Fly through the existing vault form with an explicit host, then refreshes readiness", async () => {
  const refresh = await openForm();
  expect(screen.getByLabelText("Secret name")).toHaveProperty("value", "fly");
  expect(screen.getByLabelText("Allowed host")).toHaveProperty("value", HOST);
  expect(screen.getByLabelText("Field 1 name")).toHaveProperty("value", "api_token");
  expect(screen.getByLabelText("Field 1 value")).toHaveProperty("type", "password");
  fireEvent.change(screen.getByLabelText("Field 1 value"), {target: {value: FORM_VALUE}});
  fireEvent.click(screen.getByRole("button", {name: "Create"}));
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce());
  expect(api.create).toHaveBeenCalledWith(WORKSPACE, {name: "fly", data: {api_token: FORM_VALUE, host: HOST}});
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByText("Access saved as fly.")).toBeTruthy();
  expect(document.body.textContent).not.toContain(FORM_VALUE);
  expect(api.connect).not.toHaveBeenCalled();
});

it.each(["", "https://api.fly.io/path", "api.fly.io:8443", "api.fly.io/elsewhere"])("refuses an unsafe or empty credential destination (%s)", async host => {
  await openForm();
  fireEvent.change(screen.getByLabelText("Allowed host"), {target: {value: host}});
  fireEvent.change(screen.getByLabelText("Field 1 value"), {target: {value: FORM_VALUE}});
  fireEvent.click(screen.getByRole("button", {name: "Create"}));
  await screen.findByText(host ? /Enter a host such as/ : /Enter the host allowed/);
  expect(api.create).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog")).toBeTruthy();
});

it.each(["Access denied", "Vault unavailable", "A secret with this name already exists"])("retains entered values when storage refuses: %s", async error => {
  api.create.mockResolvedValueOnce({ok: false, error});
  const refresh = await openForm();
  fireEvent.change(screen.getByLabelText("Field 1 value"), {target: {value: FORM_VALUE}});
  fireEvent.click(screen.getByRole("button", {name: "Create"}));
  await screen.findByText(new RegExp(error));
  expect(refresh).not.toHaveBeenCalled();
  expect(screen.getByLabelText("Field 1 value")).toHaveProperty("value", FORM_VALUE);
  expect(screen.queryByText("Access saved as fly.")).toBeNull();
});

it("reports a failed readiness read separately from successful secret storage", async () => {
  const refresh = await openForm(vi.fn(async () => {throw new Error("read failed");}));
  fireEvent.change(screen.getByLabelText("Field 1 value"), {target: {value: FORM_VALUE}});
  fireEvent.click(screen.getByRole("button", {name: "Create"}));
  await screen.findByText("Readiness could not be checked. Try checking again.");
  expect(api.create).toHaveBeenCalledOnce();
  refresh.mockResolvedValueOnce();
  fireEvent.click(screen.getByRole("button", {name: "Check again"}));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(refresh).toHaveBeenCalledTimes(2);
});

it("keeps credentials untouched when access options cannot be loaded", async () => {
  api.catalog.mockResolvedValue({ok: false, error: "Connections unavailable"});
  render(<SetupAccess workspace={WORKSPACE} name="fly" definition={definition} />);
  fireEvent.click(screen.getByRole("button", {name: "Connect Fly"}));
  await screen.findByText("Connections unavailable");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(api.create).not.toHaveBeenCalled();
});

it("authorizes a registered provider in a new tab without opening a plain-secret form", async () => {
  api.catalog.mockResolvedValue({ok: true, data: [{id: "zoho", display_name: "Zoho", configured: true}]});
  api.connect.mockResolvedValue({ok: true, data: {install_url: "https://accounts.zoho.test/authorize"}});
  const {popup, navigate} = authorizationWindow();
  vi.spyOn(window, "open").mockReturnValue(popup);
  render(<SetupAccess workspace={WORKSPACE} name="zoho" onRefresh={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", {name: "Connect Zoho"}));
  await screen.findByText("Finish authorizing in the new tab, then check again here.");
  expect(api.connect).toHaveBeenCalledWith("zoho", WORKSPACE);
  expect(popup.opener).toBeNull();
  expect(navigate).toHaveBeenCalledWith("https://accounts.zoho.test/authorize");
  expect(api.create).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).toBeNull();
});

it.each(["blocked", "invalid", "disabled"])("reports a provider authorization problem without pretending access was saved (%s)", async problem => {
  api.catalog.mockResolvedValue({ok: true, data: [{id: "zoho", display_name: "Zoho", configured: problem !== "disabled"}]});
  api.connect.mockResolvedValue({ok: true, data: {install_url: "http://insecure.test"}});
  const {popup, navigate} = authorizationWindow();
  vi.spyOn(window, "open").mockReturnValue(problem === "blocked" ? null : popup);
  render(<SetupAccess workspace={WORKSPACE} name="zoho" />);
  fireEvent.click(screen.getByRole("button", {name: "Connect Zoho"}));
  await screen.findByRole("alert");
  expect(api.create).not.toHaveBeenCalled();
  expect(navigate).not.toHaveBeenCalled();
  expect(screen.queryByText(/Access saved as/)).toBeNull();
});

// Value: protects=retryable provider errors; fails_when=a failed connect claims success or strands its tab;
// why_new=provider failures previously covered only insecure URLs; seam=none; no_claim=provider authorization acceptance.
it.each(["refused", "malformed", "transport"])("closes an unsuccessful authorization tab and permits retry (%s)", async failure => {
  const provider = "zoho";
  const destination = "https://accounts.zoho.test/authorize";
  const refused = "Connection could not be started";
  api.catalog.mockResolvedValue({ok: true, data: [{id: provider, display_name: "Zoho", configured: true}]});
  if (failure === "transport") api.connect.mockRejectedValueOnce(new TypeError("Connection lost"));
  else api.connect.mockResolvedValueOnce(failure === "refused" ? {ok: false, error: refused} : {ok: true, data: {install_url: "not a URL"}});
  api.connect.mockResolvedValueOnce({ok: true, data: {install_url: destination}});
  const {popup, close, navigate} = authorizationWindow();
  vi.spyOn(window, "open").mockReturnValue(popup);
  render(<SetupAccess workspace={WORKSPACE} name={provider} />);
  const connect = screen.getByRole("button", {name: "Connect Zoho"});
  fireEvent.click(connect);
  expect((await screen.findByRole("alert")).textContent).toBe(failure === "refused" ? refused : "Access could not be opened. Try again.");
  expect(close).toHaveBeenCalledOnce();
  expect(navigate).not.toHaveBeenCalled();
  expect(screen.queryByText(/Finish authorizing/)).toBeNull();
  expect(api.create).not.toHaveBeenCalled();
  fireEvent.click(connect);
  await screen.findByText("Finish authorizing in the new tab, then check again here.");
  expect(screen.queryByRole("alert")).toBeNull();
  expect(navigate).toHaveBeenCalledExactlyOnceWith(destination);
});

// Value: protects=untrusted catalog refusal; fails_when=malformed access options open a vault or provider flow;
// why_new=catalog tests covered HTTP refusal only; seam=none; no_claim=catalog transport or server validation.
it.each(["invalid", "transport"])("recovers unavailable access options without storing credentials (%s)", async failure => {
  if (failure === "transport") api.catalog.mockRejectedValueOnce(new TypeError("Connection lost"));
  else api.catalog.mockResolvedValueOnce({ok: true, data: [{id: "zoho", configured: true}]});
  api.catalog.mockResolvedValueOnce({ok: true, data: []});
  render(<SetupAccess workspace={WORKSPACE} name="fly" />);
  const connect = screen.getByRole("button", {name: "Connect Fly"});
  fireEvent.click(connect);
  expect((await screen.findByRole("alert")).textContent).toBe(failure === "transport"
    ? "Access could not be opened. Try again." : "Access options could not be loaded. Try again.");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(api.create).not.toHaveBeenCalled();
  expect(api.connect).not.toHaveBeenCalled();
  fireEvent.click(connect);
  expect(await screen.findByRole("dialog")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
});

// Value: protects=explicit host selection; fails_when=several allowed hosts silently choose a credential destination;
// why_new=the existing form supplied exactly one host; seam=none; no_claim=provider credential verification.
it("requires a destination choice when the instructions allow several hosts", async () => {
  render(<SetupAccess workspace={WORKSPACE} name="fly" definition={{...definition,
    config: {"x-agentsfleet": {network: {allow: [HOST, "metrics.fly.io"]}}}}} />);
  fireEvent.click(screen.getByRole("button", {name: "Connect Fly"}));
  expect(await screen.findByLabelText("Allowed host")).toHaveProperty("value", "");
  fireEvent.change(screen.getByLabelText("Field 1 value"), {target: {value: FORM_VALUE}});
  fireEvent.click(screen.getByRole("button", {name: "Create"}));
  await screen.findByText(/Enter the host allowed/);
  expect(api.create).not.toHaveBeenCalled();
});

// Value: protects=credentials stay local until explicit submission; fails_when=opening or cancelling a custom form writes access;
// why_new=existing forms always have a matching secret reference and network rule; seam=none; no_claim=server credential storage.
it("ignores other services' references and cancels custom access without a write", async () => {
  render(<SetupAccess workspace={WORKSPACE} name="fly" definition={{...definition,
    instructions_md: "Use ${secrets.other.key}.", config: {}}} />);
  fireEvent.click(screen.getByRole("button", {name: "Connect Fly"}));
  await screen.findByRole("dialog");
  expect(screen.getByLabelText("Field 1 name")).toHaveProperty("value", "");
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(api.create).not.toHaveBeenCalled();
  expect(api.connect).not.toHaveBeenCalled();
  expect(screen.queryByText(/Access saved as/)).toBeNull();
});

// Value: protects=readiness after provider authorization; fails_when=Check again leaves the saved fleet's access state stale;
// why_new=existing access tests replace the readiness callback; seam=none; no_claim=provider authorization or database consistency.
it("rereads saved fleet readiness after authorization without saving or starting the fleet", async () => {
  const fixture = new DraftClientFixture();
  fixture.mount(); fixture.fleet = FLEET_DETAIL;
  fixture.current = {...DRAFT, state: "closed", linked_fleet_id: FLEET_ID};
  try {
    const client = await fixture.ready(true);
    api.catalog.mockResolvedValue({ok: true, data: [{id: "github", display_name: "GitHub", configured: true}]});
    api.connect.mockResolvedValue({ok: true, data: {install_url: "https://github.test/authorize"}});
    const {popup} = authorizationWindow();
    vi.spyOn(window, "open").mockReturnValue(popup);
    render(<FleetSetup client={client} view={client.getSnapshot()} workspace={WORKSPACE} />);
    fireEvent.click(screen.getByRole("button", {name: "Connect Github"}));
    await screen.findByText("Finish authorizing in the new tab, then check again here.");
    fixture.fleet = {...FLEET_DETAIL, setup: {state: "ready", checks: []}};
    fireEvent.click(screen.getByRole("button", {name: "Check again"}));
    await waitFor(() => expect(client.getSnapshot().fleet?.setup).toEqual({state: "ready", checks: []}));
    expect(fixture.mutations()).toEqual([]);
    expect(api.create).not.toHaveBeenCalled();
  } finally {fixture.dispose();}
});
