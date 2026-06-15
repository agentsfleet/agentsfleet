// Shared helpers for parser-level cli-tree.ts tests. Builds the full
// program with a spy handler tree, silences every Command in the tree
// (commander 14 does NOT propagate exitOverride/configureOutput to
// subcommands, so a validator throwing InvalidArgumentError inside a
// leaf would otherwise call process.exit and kill the test runner).

import type { Command } from "commander";

import { buildProgram } from "../src/program/cli-tree.ts";
import type {
  ActionFrame,
  CommandHandlerFn,
  Handlers,
  ProgramState,
} from "../src/program/cli-tree-types.ts";

export const VALID_ID = "01900000-0000-7000-8000-000000000001";

export interface SpyCall {
  name: string;
  frame: ActionFrame;
}

export interface SpyTree {
  handlers: Handlers;
  calls: SpyCall[];
}

export function makeSpyTree(): SpyTree {
  const calls: SpyCall[] = [];
  const spy = (name: string): CommandHandlerFn => async (frame) => {
    calls.push({ name, frame });
    return 0;
  };
  const handlers: Handlers = {
    login: spy("login"),
    logout: spy("logout"),
    auth: {
      status: spy("auth.status"),
    },
    doctor: spy("doctor"),
    workspace: {
      add: spy("workspace.add"),
      list: spy("workspace.list"),
      use: spy("workspace.use"),
      show: spy("workspace.show"),
      credentials: spy("workspace.credentials"),
      delete: spy("workspace.delete"),
    },
    agentKey: {
      add: spy("agent-key.add"),
      list: spy("agent-key.list"),
      delete: spy("agent-key.delete"),
    },
    grant: {
      list: spy("grant.list"),
      delete: spy("grant.delete"),
    },
    tenant: {
      provider: {
        show: spy("tenant.provider.show"),
        add: spy("tenant.provider.add"),
        delete: spy("tenant.provider.delete"),
      },
    },
    billing: {
      show: spy("billing.show"),
    },
    agent: {
      install: spy("agent.install"),
      update: spy("agent.update"),
      list: spy("agent.list"),
      status: spy("agent.status"),
      stop: spy("agent.stop"),
      resume: spy("agent.resume"),
      kill: spy("agent.kill"),
      delete: spy("agent.delete"),
      logs: spy("agent.logs"),
      events: spy("agent.events"),
      steer: spy("agent.steer"),
      credential: {
        add: spy("agent.credential.add"),
        show: spy("agent.credential.show"),
        list: spy("agent.credential.list"),
        delete: spy("agent.credential.delete"),
      },
    },
    memory: {
      list: spy("memory.list"),
      search: spy("memory.search"),
    },
  };
  return { handlers, calls };
}

function silenceTree(cmd: Command): void {
  cmd.exitOverride();
  cmd.configureOutput({ writeOut: () => {}, writeErr: () => {} });
  for (const sub of cmd.commands) silenceTree(sub);
}

export interface BuiltProgram {
  program: Command;
  state: ProgramState;
}

export function buildSilent(opts: { handlers: Handlers }): BuiltProgram {
  const state: ProgramState = { exitCode: 0 };
  const program = buildProgram({ handlers: opts.handlers, version: "0.0.0-test", state });
  silenceTree(program);
  return { program, state };
}

export async function dispatch(
  argv: readonly string[],
  handlers: Handlers,
): Promise<ProgramState> {
  const { program, state } = buildSilent({ handlers });
  await program.parseAsync(argv, { from: "user" });
  return state;
}
