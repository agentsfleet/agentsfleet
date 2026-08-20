import { request } from "./client";

const CREATE_WORKSPACE_TIMEOUT_MS = 15_000;
const WORKSPACE_LIST_PAGE_LIMIT = 100;
export const WORKSPACE_NAME_MAX_CODEPOINTS = 128;
const ASCII_EDGE_WHITESPACE_PATTERN =
  /^[\u0009-\u000d\u0020]+|[\u0009-\u000d\u0020]+$/gu;
const UNICODE_WHITESPACE_ONLY_PATTERN =
  /^[\u00a0\u1680\u2000-\u200a\u202f\u205f\u3000]*$/u;
const WORKSPACE_NAME_UNSAFE_PATTERN =
  /[\u0000-\u001f\u007f-\u009f\u061c\u200e-\u200f\u2028-\u202e\u2066-\u2069]/u;

export function trimWorkspaceName(name: string): string {
  return name.replace(ASCII_EDGE_WHITESPACE_PATTERN, "");
}

export function hasWorkspaceNameContent(name: string): boolean {
  return name.length > 0 && !UNICODE_WHITESPACE_ONLY_PATTERN.test(name);
}

export function isWorkspaceNameSafe(name: string): boolean {
  return !WORKSPACE_NAME_UNSAFE_PATTERN.test(name);
}

export type TenantWorkspace = {
  id: string;
  name: string | null;
  created_at: number;
};

export type TenantWorkspaceListResponse = {
  items: TenantWorkspace[];
  tenant_id: string;
  total: number;
  next_cursor: string | null;
};

type TenantWorkspacePageResponse = {
  items: TenantWorkspace[];
  tenant_id: string;
  total: null;
  next_cursor: string | null;
};

export type CreateWorkspaceResponse = {
  workspace_id: string;
  name: string;
  tenant_id: string;
  request_id: string;
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const isNonEmptyString = (value: unknown): value is string =>
  typeof value === "string" && value.trim().length > 0;

const decodeWorkspace = (value: unknown): TenantWorkspace => {
  if (!isRecord(value)) throw new Error("workspace item is invalid");
  if (
    !isNonEmptyString(value.id) ||
    (value.name !== null && typeof value.name !== "string") ||
    !Number.isSafeInteger(value.created_at)
  ) {
    throw new Error("workspace item is invalid");
  }
  return {
    id: value.id,
    name: value.name,
    created_at: value.created_at as number,
  };
};

const decodeWorkspacePage = (value: unknown): TenantWorkspacePageResponse => {
  if (!isRecord(value)) throw new Error("workspace response is invalid");
  if (!isNonEmptyString(value.tenant_id)) {
    throw new Error("workspace response omitted tenant_id");
  }
  if (
    !Array.isArray(value.items) ||
    value.items.length > WORKSPACE_LIST_PAGE_LIMIT
  ) {
    throw new Error("workspace response omitted items");
  }
  if (value.total !== null) {
    throw new Error("workspace response returned an invalid total");
  }
  if (!Object.hasOwn(value, "next_cursor")) {
    throw new Error("workspace response omitted next_cursor");
  }
  if (
    value.next_cursor !== null &&
    !isNonEmptyString(value.next_cursor)
  ) {
    throw new Error("workspace pagination returned an invalid cursor");
  }
  return {
    items: value.items.map(decodeWorkspace),
    tenant_id: value.tenant_id,
    total: null,
    next_cursor: value.next_cursor,
  };
};

const decodeCreateWorkspace = (
  value: unknown,
  expectedName: string,
): CreateWorkspaceResponse => {
  if (
    !isRecord(value) ||
    !isNonEmptyString(value.workspace_id) ||
    value.name !== expectedName ||
    !isNonEmptyString(value.tenant_id) ||
    !isNonEmptyString(value.request_id)
  ) {
    throw new Error("workspace create response is invalid");
  }
  return {
    workspace_id: value.workspace_id,
    name: value.name,
    tenant_id: value.tenant_id,
    request_id: value.request_id,
  };
};

// GET /v1/tenants/me/workspaces — complete stable cursor walk.
// The backend resolves tenant_id from the authenticated principal.
export async function listTenantWorkspaces(
  token: string,
): Promise<TenantWorkspaceListResponse> {
  const items: TenantWorkspace[] = [];
  const seenCursors = new Set<string>();
  let tenantId: string | null = null;
  let startingAfter: string | null = null;

  do {
    const query = new URLSearchParams({
      limit: String(WORKSPACE_LIST_PAGE_LIMIT),
    });
    if (startingAfter) query.set("starting_after", startingAfter);
    const response = await request<unknown>(
      `/v1/tenants/me/workspaces?${query.toString()}`,
      { method: "GET" },
      token,
    );
    const page = decodeWorkspacePage(response);
    if (tenantId !== null && page.tenant_id !== tenantId) {
      throw new Error("workspace pagination changed tenant");
    }
    tenantId = page.tenant_id;
    items.push(...page.items);
    const nextCursor = page.next_cursor;
    if (nextCursor !== null) {
      if (seenCursors.has(nextCursor)) {
        throw new Error("workspace pagination repeated a cursor");
      }
      seenCursors.add(nextCursor);
    }
    startingAfter = nextCursor;
  } while (startingAfter !== null);

  return {
    items,
    tenant_id: tenantId,
    total: items.length,
    next_cursor: null,
  };
}

// GET /v1/tenants/me/workspaces?limit=1 — the entry redirect's read. It only
// needs the FIRST workspace (or proof there is none), so it must not pay the
// complete cursor walk the switcher needs; one page of one is the whole ask.
export async function firstTenantWorkspace(
  token: string,
): Promise<TenantWorkspace | null> {
  const response = await request<unknown>(
    "/v1/tenants/me/workspaces?limit=1",
    { method: "GET" },
    token,
  );
  const page = decodeWorkspacePage(response);
  return page.items[0] ?? null;
}

// POST /v1/workspaces — the caller supplies the tenant-unique name and the
// backend assigns the workspace ID from its authenticated tenant context.
export async function createTenantWorkspace(
  token: string,
  body: { name: string },
): Promise<CreateWorkspaceResponse> {
  const response = await request<unknown>(
    "/v1/workspaces",
    {
      method: "POST",
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(CREATE_WORKSPACE_TIMEOUT_MS),
    },
    token,
  );
  return decodeCreateWorkspace(response, body.name);
}
