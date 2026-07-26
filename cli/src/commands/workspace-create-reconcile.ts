import {
  UnexpectedError,
  type NetworkError,
  type ServerError,
} from "../errors/index.ts";

import { Effect, type Redacted } from "effect";
import {
  TENANT_WORKSPACES_PATH,
  WORKSPACES_COLLECTION_PATH,
} from "../lib/api-paths.ts";
import {
  ERR_WORKSPACE_NAME_EXISTS,
  type HttpClientShape,
} from "../services/http-client.ts";
import {
  decodeWorkspaceCreate,
  decodeWorkspacePage,
} from "./workspace-response-decoders.ts";

type RequestError = NetworkError | ServerError;
type CreateError = RequestError | UnexpectedError;

export interface WorkspaceCreateResult {
  readonly workspace_id: string;
  readonly name: string;
  readonly tenant_id: string;
  readonly created_at?: number;
  readonly request_id?: string;
}

const WORKSPACE_REQUEST_MAX_ATTEMPTS = 1;
const INVALID_CREATE_DETAIL = "workspace create response is invalid";
const INVALID_CREATE_SUGGESTION =
  "retry; if the error persists, contact support" as const;
export const WORKSPACE_CREATE_STATUS = {
  created: "created",
  reconciled: "reconciled",
} as const;

export type WorkspaceCreateOutcome =
  | {
      readonly status: typeof WORKSPACE_CREATE_STATUS.created;
      readonly workspace: WorkspaceCreateResult;
    }
  | {
      readonly status: typeof WORKSPACE_CREATE_STATUS.reconciled;
      readonly workspace: WorkspaceCreateResult;
    };

const shouldReconcile = (error: RequestError): boolean => {
  if (error._tag === "NetworkError") return true;
  return (
    error.status === 0 ||
    error.status >= 500 ||
    (error.status === 409 && error.code === ERR_WORKSPACE_NAME_EXISTS)
  );
};

const findWorkspaceByName = (
  response: unknown,
  name: string,
): WorkspaceCreateResult | null => {
  const page = decodeWorkspacePage(response);
  if (page === null || page.next_cursor !== null) return null;
  for (const item of page.items) {
    if (item.name !== name) continue;
    return {
      workspace_id: item.id,
      name,
      tenant_id: page.tenant_id,
      created_at: item.created_at,
    };
  }
  return null;
};

const reconcileCreate = (
  http: HttpClientShape,
  token: Redacted.Redacted<string>,
  name: string,
  originalError: RequestError,
): Effect.Effect<WorkspaceCreateOutcome, RequestError> => {
  const query = new URLSearchParams({ name, limit: "1" });
  return http
    .request<unknown>({
      path: `${TENANT_WORKSPACES_PATH}?${query.toString()}`,
      token,
      retry: { maxAttempts: WORKSPACE_REQUEST_MAX_ATTEMPTS },
    })
    .pipe(
      Effect.matchEffect({
        onFailure: () => Effect.fail(originalError),
        onSuccess: (response) => {
          const match = findWorkspaceByName(response, name);
          return match
            ? Effect.succeed({
                status: WORKSPACE_CREATE_STATUS.reconciled,
                workspace: match,
              })
            : Effect.fail(originalError);
        },
      }),
    );
};

export const createWorkspaceWithReconciliation = (
  http: HttpClientShape,
  token: Redacted.Redacted<string>,
  name: string,
): Effect.Effect<WorkspaceCreateOutcome, CreateError> =>
  http
    .request<unknown>({
      path: WORKSPACES_COLLECTION_PATH,
      method: "POST",
      body: { name },
      token,
      retry: { maxAttempts: WORKSPACE_REQUEST_MAX_ATTEMPTS },
    })
    .pipe(
      Effect.matchEffect({
        onFailure: (error) =>
          shouldReconcile(error)
            ? reconcileCreate(http, token, name, error)
            : Effect.fail(error),
        onSuccess: (response) => {
          const workspace = decodeWorkspaceCreate(response, name);
          return workspace === null
            ? Effect.fail(
                new UnexpectedError({
                  detail: INVALID_CREATE_DETAIL,
                  suggestion: INVALID_CREATE_SUGGESTION,
                }),
              )
            : Effect.succeed({
                status: WORKSPACE_CREATE_STATUS.created,
                workspace,
              });
        },
      }),
    );
