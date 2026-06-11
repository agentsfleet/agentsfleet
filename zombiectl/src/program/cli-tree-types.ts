// Shared types for the zombiectl command tree. Lives outside cli-tree.ts
// so the FLL cap (350L) does not block adding new verbs to the tree
// itself; consumers (cli-tree.ts, cli-tree-zombie.ts, handlers-bind.ts)
// import this module directly.

import type { Command, Help } from "commander";
import type { ParsedArgs } from "../commands/types.ts";

export interface ActionFrame {
  name: string;
  parsed: ParsedArgs;
  command: Command;
}

export type CommandHandlerFn = (
  frame: ActionFrame,
) => Promise<number | void> | number | void;

export interface AuthHandlers {
  status: CommandHandlerFn;
}

export interface WorkspaceHandlers {
  add: CommandHandlerFn;
  list: CommandHandlerFn;
  use: CommandHandlerFn;
  show: CommandHandlerFn;
  credentials: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface AgentHandlers {
  add: CommandHandlerFn;
  list: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface GrantHandlers {
  list: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface TenantProviderHandlers {
  show: CommandHandlerFn;
  add: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface TenantHandlers {
  provider: TenantProviderHandlers;
}

export interface BillingHandlers {
  show: CommandHandlerFn;
}

export interface ZombieCredentialHandlers {
  add: CommandHandlerFn;
  show: CommandHandlerFn;
  list: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface ZombieHandlers {
  install: CommandHandlerFn;
  update: CommandHandlerFn;
  list: CommandHandlerFn;
  status: CommandHandlerFn;
  stop: CommandHandlerFn;
  resume: CommandHandlerFn;
  kill: CommandHandlerFn;
  delete: CommandHandlerFn;
  logs: CommandHandlerFn;
  events: CommandHandlerFn;
  steer: CommandHandlerFn;
  credential: ZombieCredentialHandlers;
}

export interface MemoryHandlers {
  list: CommandHandlerFn;
  search: CommandHandlerFn;
}

export interface Handlers {
  login: CommandHandlerFn;
  logout: CommandHandlerFn;
  auth: AuthHandlers;
  doctor: CommandHandlerFn;
  workspace: WorkspaceHandlers;
  agent: AgentHandlers;
  grant: GrantHandlers;
  tenant: TenantHandlers;
  billing: BillingHandlers;
  zombie: ZombieHandlers;
  memory: MemoryHandlers;
}

export interface ProgramState {
  exitCode: number;
}

export interface BuildProgramOptions {
  handlers: Handlers;
  version: string;
  state: ProgramState;
  helpFactory?: (() => Help) | undefined;
}

export interface ActionDispatch {
  actionFor: (
    name: string,
    fn: (frame: ActionFrame) => Promise<void>,
  ) => (...args: unknown[]) => Promise<void>;
  runHandler: (
    state: ProgramState,
    frame: ActionFrame,
    handler: CommandHandlerFn,
  ) => Promise<void>;
}
