// Shared helpers for parser-level cli-tree.js tests. Builds the full
// program with a spy handler tree, silences every Command in the tree
// (commander 14 does NOT propagate exitOverride/configureOutput to
// subcommands, so a validator throwing InvalidArgumentError inside a
// leaf would otherwise call process.exit and kill the test runner).

import { buildProgram } from "../src/program/cli-tree.js";

export const VALID_ID = "01900000-0000-7000-8000-000000000001";

export function makeSpyTree() {
  const calls = [];
  const spy = (name) => async (frame) => {
    calls.push({ name, frame });
    return 0;
  };
  const handlers = {
    login: spy("login"),
    logout: spy("logout"),
    doctor: spy("doctor"),
    workspace: {
      add: spy("workspace.add"),
      list: spy("workspace.list"),
      use: spy("workspace.use"),
      show: spy("workspace.show"),
      credentials: spy("workspace.credentials"),
      delete: spy("workspace.delete"),
    },
    agent: {
      add: spy("agent.add"),
      list: spy("agent.list"),
      delete: spy("agent.delete"),
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
    zombie: {
      install: spy("zombie.install"),
      list: spy("zombie.list"),
      status: spy("zombie.status"),
      stop: spy("zombie.stop"),
      resume: spy("zombie.resume"),
      kill: spy("zombie.kill"),
      delete: spy("zombie.delete"),
      logs: spy("zombie.logs"),
      events: spy("zombie.events"),
      steer: spy("zombie.steer"),
      credential: {
        add: spy("zombie.credential.add"),
        show: spy("zombie.credential.show"),
        list: spy("zombie.credential.list"),
        delete: spy("zombie.credential.delete"),
      },
    },
  };
  return { handlers, calls };
}

function silenceTree(cmd) {
  cmd.exitOverride();
  cmd.configureOutput({ writeOut: () => {}, writeErr: () => {} });
  for (const sub of cmd.commands) silenceTree(sub);
}

export function buildSilent({ handlers } = {}) {
  const state = { exitCode: 0 };
  const program = buildProgram({ handlers, version: "0.0.0-test", state });
  silenceTree(program);
  return { program, state };
}

export async function dispatch(argv, handlers) {
  const { program, state } = buildSilent({ handlers });
  await program.parseAsync(argv, { from: "user" });
  return state;
}
