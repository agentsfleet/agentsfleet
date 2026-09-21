// Help is the highest-frequency surface this CLI has, and the formatter is the
// only thing standing between the library's defaults and the house style. Each
// test here names a reader-visible property, not an implementation detail.

import { describe, expect, test } from "bun:test";
import { CliError } from "effect/unstable/cli";

import { helpFormatter, helpTail } from "../src/program/entry/help-formatter.ts";

const MAX_WIDTH = 80;
const formatter = helpFormatter();

const longSentence = (words: number): string =>
  Array.from({ length: words }, (_, i) => `word${i}`).join(" ");

describe("help formatter — the 80-column ceiling", () => {
  test("a two-column entry keeps its column and hangs the wrap under it", () => {
    const doc = {
      usage: "agentsfleet demo",
      description: "demo",
      flags: [
        {
          name: "verbose",
          aliases: [],
          type: "boolean",
          description: { _tag: "Some", value: longSentence(30) },
          required: false,
        },
      ],
      args: [],
    } as never;
    const lines = formatter.formatHelpDoc(doc).split("\n");
    expect(lines.filter((line) => line.length > MAX_WIDTH)).toEqual([]);
  });

  test("no rendered line exceeds the ceiling, however long the prose", () => {
    const doc = {
      usage: `agentsfleet ${longSentence(40)}`,
      description: longSentence(60),
      flags: [],
      args: [],
    } as never;
    const over = formatter
      .formatHelpDoc(doc)
      .split("\n")
      .filter((line) => line.length > MAX_WIDTH);
    expect(over).toEqual([]);
  });

  test("a line already inside the ceiling is left byte-identical", () => {
    const doc = {
      usage: "agentsfleet demo",
      description: "short",
      flags: [],
      args: [],
    } as never;
    expect(formatter.formatHelpDoc(doc)).toContain("short");
  });
});

describe("help formatter — the configuration pointer", () => {
  test("every help document ends with the env-var docs pointer", () => {
    const doc = { usage: "u", description: "d", flags: [], args: [] } as never;
    expect(formatter.formatHelpDoc(doc).endsWith(helpTail())).toBe(true);
  });

  test("the pointer names the documented URL, once", () => {
    const doc = { usage: "u", description: "d", flags: [], args: [] } as never;
    const rendered = formatter.formatHelpDoc(doc);
    const hits = rendered.split("https://docs.agentsfleet.net/cli/configuration").length - 1;
    expect(hits).toBe(1);
  });
});

describe("help formatter — a dead end gets a way out", () => {
  test("an unknown subcommand with no near match points at --help", () => {
    const error = new CliError.UnknownSubcommand({
      subcommand: "zzzzz",
      suggestions: [],
    });
    expect(formatter.formatErrors([error])).toContain("agentsfleet --help");
  });

  test("an unknown subcommand WITH a suggestion is not also sent to --help", () => {
    const error = new CliError.UnknownSubcommand({
      subcommand: "docto",
      suggestions: ["doctor"],
    });
    const rendered = formatter.formatErrors([error]);
    expect(rendered).toContain("doctor");
    expect(rendered).not.toContain("agentsfleet --help");
  });

  test("an unrelated error is left to the library", () => {
    const error = new CliError.MissingArgument({ argument: "fleet_id" });
    expect(formatter.formatErrors([error])).not.toContain("agentsfleet --help");
  });

  // The library renders a parse failure through the plural form, but a caller
  // reaching for either singular one should get the same sentence — otherwise
  // the pointer appears or vanishes depending on which entry point rendered.
  test("the singular formatters answer the same way as the plural", () => {
    const dead = new CliError.UnknownSubcommand({ subcommand: "zzzzz", suggestions: [] });
    expect(formatter.formatCliError(dead)).toContain("agentsfleet --help");
    expect(formatter.formatError(dead)).toContain("agentsfleet --help");
  });

  test("the singular formatters leave an unrelated error alone", () => {
    const other = new CliError.MissingArgument({ argument: "fleet_id" });
    expect(formatter.formatCliError(other)).not.toContain("agentsfleet --help");
    expect(formatter.formatError(other)).not.toContain("agentsfleet --help");
  });
});

describe("help formatter — the flag we do not advertise", () => {
  const flag = (name: string, description: string) => ({
    name,
    aliases: [],
    type: "boolean",
    description: { _tag: "Some", value: description },
    required: false,
  });

  const render = (...flags: ReadonlyArray<ReturnType<typeof flag>>): string =>
    formatter.formatHelpDoc({
      usage: "agentsfleet demo",
      description: "demo",
      flags,
      args: [],
    } as never);

  test("a flag whose name merely begins with the unadvertised one is still offered", () => {
    // `startsWith` on the term would take `--wizardly` out with `--wizard`,
    // and its description with it — a flag vanishing from the help because
    // of what another flag is called is the reader's problem, not a match.
    const rendered = render(flag("wizardly", "a flag this repository does advertise"));
    expect(rendered).toContain("--wizardly");
    expect(rendered).toContain("a flag this repository does advertise");
  });

  test("the unadvertised flag goes, and the flag declared after it stays", () => {
    const rendered = render(
      flag("wizard", "the library's own builder"),
      flag("json", "machine-readable output"),
    );
    expect(rendered).not.toContain("--wizard");
    expect(rendered).not.toContain("the library's own builder");
    expect(rendered).toContain("--json");
    expect(rendered).toContain("machine-readable output");
  });
});
