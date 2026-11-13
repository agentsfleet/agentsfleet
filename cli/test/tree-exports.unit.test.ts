// No tree module exports a command value: the tree is built per invocation
// from factories, so nothing a run decides can be carried into the next one.

import { describe, expect, test } from "bun:test";
import path from "node:path";

const TREE_DIR = path.join(import.meta.dir, "..", "src", "program", "tree");

// `export const x = Command.make(...)`, `export const x = Command.unlisted(...)`,
// or an exported array literal holding commands.
const COMMAND_CONSTANT = /^export const \w+\s*(?::[^=]+)?=\s*(?:Command\.|\[[^\]]*Command)/m;

const commandConstantsIn = (source: string): boolean => COMMAND_CONSTANT.test(source);

describe("tree_modules_export_no_command_constants", () => {
  test("the pattern detects a command constant (positive control)", () => {
    expect(commandConstantsIn('export const rootCommand = Command.make("agentsfleet").pipe(')).toBe(true);
    expect(commandConstantsIn("export const groups = [listCommand, showCommand];")).toBe(true);
    expect(commandConstantsIn('export const fleetCommand = () => Command.make("fleet")')).toBe(false);
  });

  test("every tree module exports factories, not command values", async () => {
    const files = await Array.fromAsync(new Bun.Glob("*.ts").scan({ cwd: TREE_DIR }));
    expect(files.length).toBeGreaterThan(10);
    for (const file of files) {
      const source = await Bun.file(path.join(TREE_DIR, file)).text();
      expect({ file, exportsCommand: commandConstantsIn(source) }).toEqual({ file, exportsCommand: false });
    }
  });
});
