// One bounded request per page; mutations use the same private draft endpoints.

import { Effect, Schema } from "effect";
import { CLI_ERROR_TAG, ServerError, ValidationError } from "../../errors/index.ts";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { HttpClient } from "../../services/http-client.ts";
import { Input } from "../../services/input.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { Stdin } from "../../services/stdin.ts";
import { EMPTY_CELL, ago } from "../../output/index.ts";
import { requireValidId, resolveAuthToken, resolveWorkspaceId, WORKSPACE_FLAG } from "../workspace-guards.ts";
import { createTransport, IF_MATCH } from "./transport.ts";
import { CHAT_TEXT_KIND, CHAT_PAGE_MAX, chatStatus, chatText, chatVersions, decodeChatPage, decodeChatTitle } from "./history-schema.ts";

type WorkspaceOptions = {readonly workspace?: string | undefined};
export type ChatListOptions = WorkspaceOptions & {
  readonly search?: string | undefined; readonly cursor?: string | undefined; readonly limit?: string | undefined;
  readonly state?: string | undefined; readonly fleet?: string | undefined;
};
const DEFAULT_LIMIT = 50;
const DRAFT_ID_FIELD = "draft_id";
const LIST_USAGE = "agentsfleet chat list --help";
const BUSY_CODE = "UZ-AUTHOR-002";
const object = Schema.Record(Schema.String, Schema.Unknown);

const historyTransport = Effect.fn("chat.history.transport")(function* (options: WorkspaceOptions) {
  const workspace = yield* resolveWorkspaceId(options.workspace, WORKSPACE_FLAG);
  const token = yield* resolveAuthToken;
  return createTransport(yield* HttpClient, workspace, token);
});

const listQuery = Effect.fn("chat.history.query")(function* (options: ChatListOptions) {
  const limit = options.limit ?? String(DEFAULT_LIMIT);
  if (!/^\d+$/u.test(limit) || Number(limit) < 1 || Number(limit) > CHAT_PAGE_MAX) return yield* Effect.fail(new ValidationError({
    detail: `--limit must be an integer between 1 and ${CHAT_PAGE_MAX}`, suggestion: LIST_USAGE,
  }));
  const query = new URLSearchParams({limit});
  if (options.search !== undefined) query.set("q", yield* chatText(options.search, CHAT_TEXT_KIND.search));
  if (options.cursor !== undefined) {
    if (!options.cursor) return yield* Effect.fail(new ValidationError({detail: "--cursor must not be empty", suggestion: LIST_USAGE}));
    query.set("starting_after", options.cursor);
  }
  if (options.state !== undefined) query.set("state", options.state);
  if (options.fleet !== undefined) query.set("linked_fleet_id", yield* requireValidId(options.fleet, "fleet_id", LIST_USAGE));
  return query;
});

export const chatListEffect = Effect.fn("chat.list")(function* (options: ChatListOptions) {
  const query = yield* listQuery(options);
  const transport = yield* historyTransport(options);
  const output = yield* Output;
  const body = yield* transport.call({path: `${transport.drafts}?${query}`},
    object);
  const page = yield* decodeChatPage(body);
  if (output.format === OUTPUT_FORMAT.json) return yield* output.printJson(body);
  if (!page.items.length) yield* output.info("No setup chats match this list.");
  else yield* output.printTable([
    {key: "title", label: "TITLE"}, {key: "id", label: "CHAT"}, {key: "status", label: "STATUS"},
    {key: "versions", label: "VERSIONS"}, {key: "updated", label: "UPDATED"},
  ], page.items.map(row => ({title: row.title, id: row.id, status: chatStatus(row),
    versions: chatVersions(row) || EMPTY_CELL, updated: ago(row.updated_at)})));
  if (page.next_cursor !== null) yield* output.info(`More chats available. Repeat this list with --cursor ${page.next_cursor} and the same filters.`);
});

export const chatRenameEffect = Effect.fn("chat.rename")(function* (draftId: string, title: string, options: WorkspaceOptions) {
  const id = yield* requireValidId(draftId, DRAFT_ID_FIELD, "agentsfleet chat rename <id> <title>");
  const name = yield* chatText(title, CHAT_TEXT_KIND.title);
  const transport = yield* historyTransport(options);
  const path = `${transport.drafts}/${encodeURIComponent(id)}`;
  const before = yield* transport.call({path}, object);
  const current = yield* decodeChatTitle(id, before);
  const body = yield* transport.call({path, method: HTTP_METHOD.patch, headers: {[IF_MATCH]: current.etag},
    body: {title: name}, retry: null}, object).pipe(Effect.mapError(error =>
    error._tag === CLI_ERROR_TAG.server && error.code === BUSY_CODE ? new ServerError({
      code: error.code, status: error.status, requestId: error.requestId,
      detail: "This setup chat is busy or closed and cannot be renamed",
      suggestion: "wait for its reply or save to finish, then rename an idle chat",
    }) : error));
  const changed = yield* decodeChatTitle(id, body);
  yield* (yield* Output).success(`Renamed setup chat to ${changed.title}`, body);
});

export const chatDeleteEffect = Effect.fn("chat.delete")(function* (draftId: string, options: WorkspaceOptions & {readonly yes: boolean}) {
  const id = yield* requireValidId(draftId, DRAFT_ID_FIELD, "agentsfleet chat delete <id> --yes");
  const output = yield* Output;
  if (!options.yes) {
    const stdin = yield* Stdin;
    if (!stdin.isTTY || output.format === OUTPUT_FORMAT.json) return yield* Effect.fail(new ValidationError({
      detail: "Deleting a setup chat needs confirmation", suggestion: "pass --yes to delete this private chat; saved fleets keep running",
    }));
    const answer = yield* (yield* Input).readLine(`Delete setup chat ${id}? Saved fleets keep running. [y/N] `);
    if (answer?.trim().toLowerCase() !== "y" && answer?.trim().toLowerCase() !== "yes") {
      return yield* output.info("Chat deletion cancelled.");
    }
  }
  const transport = yield* historyTransport(options);
  yield* transport.empty({path: `${transport.drafts}/${encodeURIComponent(id)}`, method: HTTP_METHOD.delete, retry: null});
  yield* output.success(`Deleted setup chat ${id}. Saved fleets keep running.`, {draft_id: id, deleted: true});
});
