import { request, requestWithEtag } from "./client";
import { PublishedSchema } from "./draft-types";
import { matchesResourceEtag } from "./etag";
import { PlatformEntriesSchema, PlatformEntrySchema, type PlatformPatch, type StagePlatform } from "./platform-library-types";

const PLATFORM_PATH = "/v1/admin/fleet-libraries";
const libraryPath = (workspace: string) => `/v1/workspaces/${encodeURIComponent(workspace)}/fleet-libraries`;
const platformPath = (id: string) => `${PLATFORM_PATH}/${encodeURIComponent(id)}`;

export async function removeWorkspaceLibraryEntry(workspace: string, id: string, token: string): Promise<void> {
  await request<void>(`${libraryPath(workspace)}/${encodeURIComponent(id)}`, {method: "DELETE"}, token);
}

export async function listPlatformFleetLibrary(token: string) {
  return PlatformEntriesSchema.parse(await request<unknown>(PLATFORM_PATH, {method: "GET"}, token));
}

export async function stagePlatformFleetLibrary(body: StagePlatform, operation: string, token: string) {
  return PublishedSchema.parse(await request<unknown>(PLATFORM_PATH, {
    method: "POST", body: JSON.stringify(body), headers: {"Idempotency-Key": operation},
  }, token));
}

export async function patchPlatformFleetLibraryEntry(id: string, body: PlatformPatch, ifMatch: string, token: string) {
  const response = await requestWithEtag<unknown>(platformPath(id), {
    method: "PATCH", body: JSON.stringify(body), headers: {"If-Match": ifMatch},
  }, token);
  const entry = PlatformEntrySchema.parse(response.data);
  if (!matchesResourceEtag(entry.etag, response.etag)) throw new Error("Platform Library body and response ETag disagree");
  return entry;
}

export async function deletePlatformFleetLibraryEntry(id: string, ifMatch: string, token: string): Promise<void> {
  await request<void>(platformPath(id), {method: "DELETE", headers: {"If-Match": ifMatch}}, token);
}
