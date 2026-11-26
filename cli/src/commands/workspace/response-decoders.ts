import { isRecord, isString } from "../../lib/guards.ts";
import { isInstant } from "../../lib/instant.ts";

export interface WorkspaceWireItem {
  readonly id: string;
  readonly name: string | null;
  readonly created_at: string;
}

export interface WorkspacePageResponse {
  readonly items: WorkspaceWireItem[];
  readonly tenant_id: string;
  readonly total: null;
  readonly next_cursor: string | null;
}

export interface WorkspaceCreateResponse {
  readonly workspace_id: string;
  readonly name: string;
  readonly tenant_id: string;
  readonly request_id: string;
}

const isNonEmptyString = (value: unknown): value is string =>
  isString(value) && value.trim().length > 0;

const decodeWorkspaceItem = (value: unknown): WorkspaceWireItem | null => {
  if (!isRecord(value)) return null;
  if (
    !isNonEmptyString(value.id) ||
    (value.name !== null && !isString(value.name)) ||
    !isInstant(value.created_at)
  ) {
    return null;
  }
  return {
    id: value.id,
    name: value.name,
    created_at: value.created_at,
  };
};

export const decodeWorkspacePage = (
  value: unknown,
): WorkspacePageResponse | null => {
  if (!isRecord(value)) return null;
  if (
    !isNonEmptyString(value.tenant_id) ||
    !Array.isArray(value.items) ||
    value.total !== null ||
    !Object.hasOwn(value, "next_cursor")
  ) {
    return null;
  }
  if (value.next_cursor !== null && !isNonEmptyString(value.next_cursor)) {
    return null;
  }
  const items: WorkspaceWireItem[] = [];
  for (const raw of value.items) {
    const item = decodeWorkspaceItem(raw);
    if (item === null) return null;
    items.push(item);
  }
  return {
    items,
    tenant_id: value.tenant_id,
    total: null,
    next_cursor: value.next_cursor,
  };
};

export const decodeWorkspaceCreate = (
  value: unknown,
  expectedName: string,
): WorkspaceCreateResponse | null => {
  if (!isRecord(value)) return null;
  if (
    !isNonEmptyString(value.workspace_id) ||
    value.name !== expectedName ||
    !isNonEmptyString(value.tenant_id) ||
    !isNonEmptyString(value.request_id)
  ) {
    return null;
  }
  return {
    workspace_id: value.workspace_id,
    name: value.name,
    tenant_id: value.tenant_id,
    request_id: value.request_id,
  };
};
