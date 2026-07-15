// Shared types for the agentsfleet command tree. Lives outside cli-tree.ts
// so the FLL cap (350L) does not block adding new verbs to the tree
// itself; consumers (cli-tree.ts, cli-tree-fleet.ts, handlers-bind.ts)
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
  create: CommandHandlerFn;
  list: CommandHandlerFn;
  use: CommandHandlerFn;
  show: CommandHandlerFn;
  secrets: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface FleetKeyHandlers {
  create: CommandHandlerFn;
  list: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface ApiKeyHandlers {
  create: CommandHandlerFn;
  list: CommandHandlerFn;
  revoke: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface ConnectorHandlers {
  list: CommandHandlerFn;
  status: CommandHandlerFn;
}

export interface GrantHandlers {
  list: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface ScheduleHandlers {
  add: CommandHandlerFn;
  list: CommandHandlerFn;
  update: CommandHandlerFn;
  rm: CommandHandlerFn;
  status: CommandHandlerFn;
  sync: CommandHandlerFn;
}

export interface TenantProviderHandlers {
  show: CommandHandlerFn;
  create: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface TenantHandlers {
  provider: TenantProviderHandlers;
}

export interface BillingHandlers {
  show: CommandHandlerFn;
}

export interface FleetSecretHandlers {
  create: CommandHandlerFn;
  show: CommandHandlerFn;
  list: CommandHandlerFn;
  delete: CommandHandlerFn;
}

export interface FleetHandlers {
  library: CommandHandlerFn;
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
  secret: FleetSecretHandlers;
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
  fleetKey: FleetKeyHandlers;
  apiKey: ApiKeyHandlers;
  connector: ConnectorHandlers;
  grant: GrantHandlers;
  schedule: ScheduleHandlers;
  tenant: TenantHandlers;
  billing: BillingHandlers;
  fleet: FleetHandlers;
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
