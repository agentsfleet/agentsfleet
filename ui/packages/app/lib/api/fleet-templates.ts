import { cache } from "react";
import { request } from "./client";
import type {
  FleetTemplateGalleryResponse,
  OnboardedTemplate,
  OnboardTemplateRequest,
} from "../types";

const workspaceFleetTemplatesPath = (workspaceId: string) =>
  `/v1/workspaces/${workspaceId}/fleet-templates`;

// Fleet template gallery client. Mirrors src/agentsfleetd/http/routes.zig:
//   GET /v1/workspaces/{ws}/fleet-templates  (platform ∪ own-tenant templates)
//
// The gallery returns the union of the platform catalog and the caller-
// workspace's own tenant templates — and nothing from another workspace. Each
// entry carries `visibility`, so the install flow keys the create body off the
// chosen tier (platform_template_id vs tenant_template_id). Metadata only — the
// canonical bundle bytes live in R2, never in the response.
export async function listWorkspaceFleetTemplates(
  workspaceId: string,
  token: string,
): Promise<FleetTemplateGalleryResponse> {
  return request<FleetTemplateGalleryResponse>(
    workspaceFleetTemplatesPath(workspaceId),
    { method: "GET" },
    token,
  );
}

export async function onboardWorkspaceFleetTemplate(
  workspaceId: string,
  body: OnboardTemplateRequest,
  token: string,
): Promise<OnboardedTemplate> {
  return request<OnboardedTemplate>(
    workspaceFleetTemplatesPath(workspaceId),
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

// Per-request deduped gallery read. The gallery is rarely-changing metadata;
// React's cache() collapses repeat reads within one RSC render (the dashboard
// gallery and /fleets/new both list templates) to a single round-trip.
// Server-only — cache() is a React Server Component primitive.
export const listWorkspaceFleetTemplatesCached = cache(listWorkspaceFleetTemplates);
