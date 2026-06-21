// Memory subtree of the agentsfleet command program — the read-only window
// into a fleet's durable memory. Pure construction; caller
// (cli-tree.ts#buildProgram) passes the parent program, the wired handler
// map, and the shared `state` object runHandler writes exit codes onto.
// Own file per the cli-tree-fleet.ts precedent so the LENGTH GATE on
// cli-tree.ts keeps headroom as verbs accrue.
//
// Top-level `memory` noun (the grant/fleet convention for nouns acting on
// per-fleet resources via flags); no write verb exists by architecture —
// the tenant memory plane is read-only.

import type { Command } from "commander";
import { parseIdOption, parseIntOption } from "./validators.ts";
import {
  DEFAULT_LIST_LIMIT,
  DEFAULT_RECALL_LIMIT,
  MAX_RECALL_LIMIT,
} from "../constants/memory-limits.ts";
import type { ActionDispatch, Handlers, ProgramState } from "./cli-tree-types.ts";

// Declared before use (owner preference for this file; the sibling tree
// files keep their bottom-declared blocks).
const FLAG_LIMIT_N = "--limit <n>" as const;
const FLAG_WORKSPACE_ID = "--workspace <id>" as const;
const FLAG_FLEET_ID = "--fleet <id>" as const;
const MAX_ENTRIES = "Max entries to return" as const;
const WORKSPACE_ID = "Workspace ID" as const;
const FLEET_ID = "Fleet ID" as const;

const MEMORY_LIMIT_BOUNDS = { min: 1, max: MAX_RECALL_LIMIT };

export function buildMemoryTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  const memory = program
    .command("memory")
    .description("Inspect a Fleet's durable memory (read-only)");

  memory
    .command("list")
    .description(`List entries newest-first (server default ${DEFAULT_LIST_LIMIT}, cap ${MAX_RECALL_LIMIT})`)
    .option(FLAG_FLEET_ID, FLEET_ID, parseIdOption)
    .option("--category <name>", "Filter by category")
    .option(FLAG_LIMIT_N, MAX_ENTRIES, parseIntOption(MEMORY_LIMIT_BOUNDS))
    .option(FLAG_WORKSPACE_ID, WORKSPACE_ID, parseIdOption)
    .action(actionFor("memory.list", (frame) => runHandler(state, frame, handlers.memory.list)));

  memory
    .command("search <query>")
    .description(`Substring-search keys and content (server default ${DEFAULT_RECALL_LIMIT}, cap ${MAX_RECALL_LIMIT})`)
    .option(FLAG_FLEET_ID, FLEET_ID, parseIdOption)
    .option(FLAG_LIMIT_N, MAX_ENTRIES, parseIntOption(MEMORY_LIMIT_BOUNDS))
    .option(FLAG_WORKSPACE_ID, WORKSPACE_ID, parseIdOption)
    .action(actionFor("memory.search", (frame) => runHandler(state, frame, handlers.memory.search)));
}
