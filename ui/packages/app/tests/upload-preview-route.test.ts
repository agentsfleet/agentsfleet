// @vitest-environment node
import { beforeEach, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api/errors";
import { POST } from "@/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/uploads/route";

const mocks = vi.hoisted(() => ({credential: vi.fn(), getFleet: vi.fn()}));
vi.mock("@/lib/auth/credential", () => ({credential: mocks.credential}));
vi.mock("@/lib/api/fleets", () => ({getFleet: mocks.getFleet}));
const WORKSPACE = "0197b580-7c00-7000-8000-000000000001";
const FLEET = "0197b580-7c00-7000-8000-000000000002";
const PARAMS = {params: Promise.resolve({workspaceId: WORKSPACE, fleetId: FLEET})};

beforeEach(() => {mocks.credential.mockReset().mockResolvedValue("session"); mocks.getFleet.mockReset().mockResolvedValue({fleet: {status: "active"}});});

it("acknowledges the preview after fleet authorization without reading file content", async () => {
  const request = upload(new File(["demo"], "notes 東京.txt"));
  const response = await POST(request, PARAMS);
  const body = await response.json();
  expect(response.status).toBe(200);
  expect(body).toEqual({preview: true, stored: false,
    reply: "Upload preview accepted. The file was not read, stored, or analyzed."});
  expect(response.headers.get("cache-control")).toBe("no-store");
  expect(mocks.getFleet).toHaveBeenCalledWith(WORKSPACE, FLEET, "session");
  expect(request.bodyUsed).toBe(false);
});

it("does not read file bytes without authentication", async () => {
  mocks.credential.mockResolvedValue(null);
  const request = upload();
  const response = await POST(request, PARAMS);
  expect(response.status).toBe(401);
  expect(await response.json()).toEqual({error: "Sign in before uploading a preview."});
  expect(request.bodyUsed).toBe(false);
  expect(mocks.getFleet).not.toHaveBeenCalled();
});

it.each([401, 403, 404])("preserves a %s fleet-access refusal before reading the file", async status => {
  mocks.getFleet.mockRejectedValue(new ApiError("private backend detail", status, "ACCESS"));
  const request = upload();
  const response = await POST(request, PARAMS);
  expect(response.status).toBe(status);
  expect(await response.json()).toEqual({error: "This fleet could not be authorized for an upload preview."});
  expect(request.bodyUsed).toBe(false);
});

it("refuses archive writes", async () => {
  mocks.getFleet.mockResolvedValue({fleet: {status: "archived"}});
  const response = await POST(upload(), PARAMS);
  expect(response.status).toBe(409);
  expect(await response.json()).toEqual({error: "Archived fleets are read-only."});
});

it("refuses a foreign origin before resolving a credential", async () => {
  const response = await POST(upload(undefined, "https://foreign.test"), PARAMS);
  expect(response.status).toBe(403);
  expect(mocks.credential).not.toHaveBeenCalled();
});

it("does not consume or parse a request stream while upload processing is a stub", async () => {
  const pull = vi.fn();
  const body = new ReadableStream<Uint8Array>({pull}, {highWaterMark: 0});
  const init: RequestInit & {duplex: "half"} = {method: "POST", body, duplex: "half",
    headers: {origin: "http://localhost", "content-type": "application/octet-stream"}};
  const request = new Request("http://localhost/upload", init);
  const response = await POST(request, PARAMS);
  expect(response.status).toBe(200);
  expect(request.bodyUsed).toBe(false);
  expect(pull).not.toHaveBeenCalled();
  await body.cancel();
});

it("does not acknowledge success when fleet access cannot be checked", async () => {
  mocks.getFleet.mockRejectedValue(new Error("Unreachable"));
  const request = upload();
  const response = await POST(request, PARAMS);
  expect(response.status).toBe(503);
  expect(await response.json()).toEqual({error: "This fleet could not be authorized for an upload preview."});
  expect(request.bodyUsed).toBe(false);
});

it("refuses path normalization input before authorization", async () => {
  const response = await POST(upload(), {params: Promise.resolve({workspaceId: "..", fleetId: FLEET})});
  expect(response.status).toBe(400);
  expect(mocks.getFleet).not.toHaveBeenCalled();
});

function upload(file = new File(["demo"], "notes.txt"), origin = "http://localhost") {
  const form = new FormData(); form.set("file", file);
  return new Request("http://localhost/upload", {method: "POST", headers: {origin}, body: form});
}
