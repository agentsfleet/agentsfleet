import { Option } from "effect";
import { Argument, Command, Flag } from "effect/cli";
import { chatDeleteEffect, chatListEffect, chatRenameEffect } from "../../commands/chat/history.ts";
import { DRAFT_STATE } from "../../commands/chat/protocol.ts";
import { cursorFlag, fleetFlag, METAVAR, workspaceFlag } from "./flags.ts";
import { guardedHandler } from "./guarded-handler.ts";

const opt = Option.getOrUndefined;
const draftId = Argument.String("draft-id").pipe(Argument.withDescription("Private setup chat identifier"));
const textFlag = (name: string, description: string, metavar: string) => Flag.String(name).pipe(
  Flag.withDescription(description), Flag.withMetavar(metavar), Flag.optional,
);
const list = Command.make("list", {
  search: textFlag("search", "Search your chat titles and message text, 1..200 characters", METAVAR.text),
  limit: textFlag("limit", "Page size, 1..100 (default 50)", METAVAR.count), cursor: cursorFlag,
  state: Flag.Literals("state", Object.values(DRAFT_STATE)).pipe(Flag.withDescription("Filter by editing, saving or closed"), Flag.optional),
  fleet: fleetFlag, workspace: workspaceFlag,
}).pipe(Command.withDescription("List or search your private setup chats (one page)"),
  guardedHandler(flags => chatListEffect({search: opt(flags.search), limit: opt(flags.limit), cursor: opt(flags.cursor),
    state: opt(flags.state), fleet: opt(flags.fleet), workspace: opt(flags.workspace)})));

const rename = Command.make("rename", {draftId, title: Argument.String("title"), workspace: workspaceFlag}).pipe(
  Command.withDescription("Rename an idle private setup chat; saved fleet names stay the same"),
  guardedHandler(flags => chatRenameEffect(flags.draftId, flags.title, {workspace: opt(flags.workspace)})),
);
const remove = Command.make("delete", {draftId, workspace: workspaceFlag,
  yes: Flag.Boolean("yes").pipe(Flag.withDescription("Confirm deletion; saved fleets keep running"), Flag.withDefault(false)),
}).pipe(Command.withDescription("Delete a private setup chat; saved fleets keep running"),
  guardedHandler(flags => chatDeleteEffect(flags.draftId, {workspace: opt(flags.workspace), yes: flags.yes})));

export const chatHistoryCommands = [list, rename, remove] as const;
