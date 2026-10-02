import { request } from "./client";
import { isNonEmptyString, isRecord } from "./decode";
import { ACCOUNT_ROLE, type AccountRole } from "./workspaces-types";

const CREATE_WORKSPACE_TIMEOUT_MS = 15_000;

export const WORKSPACE_LIST_PAGE_LIMIT = 100;

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";

/** The account a workspace belongs to, which is how the switcher groups. */
export type WorkspaceAccount = {
  tenant_id: string;
  owner_name: string;
};

export type TenantWorkspace = {
  id: string;
  name: string | null;
  created_at: number;
  account: WorkspaceAccount;
  role: AccountRole;
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

const ACCOUNT_ROLES: ReadonlySet<string> = new Set(Object.values(ACCOUNT_ROLE));

export const isAccountRole = (value: unknown): value is AccountRole =>
  typeof value === "string" && ACCOUNT_ROLES.has(value);

export const decodeWorkspaceAccount = (value: unknown): WorkspaceAccount => {
  if (
    !isRecord(value) ||
    !isNonEmptyString(value.tenant_id) ||
    !isNonEmptyString(value.owner_name)
  ) {
    throw new Error("workspace account is invalid");
  }
  return { tenant_id: value.tenant_id, owner_name: value.owner_name };
};

const decodeWorkspace = (value: unknown): TenantWorkspace => {
  if (!isRecord(value)) throw new Error("workspace item is invalid");
  if (
    !isNonEmptyString(value.id) ||
    (value.name !== null && typeof value.name !== "string") ||
    !Number.isSafeInteger(value.created_at) ||
    !isAccountRole(value.role)
  ) {
    throw new Error("workspace item is invalid");
  }
  return {
    id: value.id,
    name: value.name,
    created_at: value.created_at as number,
    account: decodeWorkspaceAccount(value.account),
    role: value.role,
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
    !isNonEmptyString(value.name) ||
    (expectedName !== "" && value.name !== expectedName) ||
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

// GET /v1/tenants/me/workspaces, one page at a time along the stable cursor,
// handing each page to `visit` until it answers true or the pages end. The
// backend resolves tenant_id from the authenticated principal; a page that
// names another tenant, or a cursor seen before, ends the walk with an error.
// Resolves to that tenant: the walk always reads at least one page.
async function walkTenantWorkspacePages(
  token: string,
  visit: (page: TenantWorkspacePageResponse) => boolean,
): Promise<string> {
  const seenCursors = new Set<string>();
  let tenantId: string | null = null;
  let startingAfter: string | null = null;
  let done = false;

  do {
    const query = new URLSearchParams({
      limit: String(WORKSPACE_LIST_PAGE_LIMIT),
    });
    if (startingAfter) query.set("starting_after", startingAfter);
    const response = await request<unknown>(
      `${TENANT_WORKSPACES_PATH}?${query.toString()}`,
      { method: "GET" },
      token,
    );
    const page = decodeWorkspacePage(response);
    if (tenantId !== null && page.tenant_id !== tenantId) {
      throw new Error("workspace pagination changed tenant");
    }
    tenantId = page.tenant_id;
    done = visit(page);
    const nextCursor = page.next_cursor;
    if (nextCursor !== null) {
      if (seenCursors.has(nextCursor)) {
        throw new Error("workspace pagination repeated a cursor");
      }
      seenCursors.add(nextCursor);
    }
    startingAfter = nextCursor;
  } while (!done && startingAfter !== null);

  return tenantId;
}

// The complete walk the switcher needs.
export async function listTenantWorkspaces(
  token: string,
): Promise<TenantWorkspaceListResponse> {
  const items: TenantWorkspace[] = [];
  const tenantId = await walkTenantWorkspacePages(token, (page) => {
    items.push(...page.items);
    return false;
  });
  return {
    items,
    tenant_id: tenantId,
    total: items.length,
    next_cursor: null,
  };
}

// The entry redirect's read. The list spans every account the caller joined,
// oldest first, so an invitee's first rows are often the inviter's older
// workspaces; the caller's own workspace wins. The walk stops at the first page
// holding one, which is page one for nearly everyone, and the first row stands
// in for a caller who owns none.
export async function firstTenantWorkspace(
  token: string,
): Promise<TenantWorkspace | null> {
  const found: { owned: TenantWorkspace | null; first: TenantWorkspace | null } = {
    owned: null,
    first: null,
  };
  await walkTenantWorkspacePages(token, ({ items }) => {
    found.owned = items.find((workspace) => workspace.role === ACCOUNT_ROLE.owner) ?? null;
    found.first ??= items[0] ?? null;
    return found.owned !== null;
  });
  return found.owned ?? found.first;
}

// POST /v1/workspaces — a blank name asks the backend to generate one.
// The backend assigns the workspace ID from its authenticated tenant context.
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
