import { existsSync } from "node:fs";
import { isAbsolute, join, normalize, resolve } from "node:path";

type MarkdownElement = { type?: unknown; props: { children?: MarkdownNode[] } };
type MarkdownNode = string | MarkdownElement;
type Section = { title: string; nodes: MarkdownNode[] };
type Commands = Record<string, string[][]>;

const CONFIG_PATH = ".oracle/orly.json";
const SPEC_LINE_LIMIT = 320;
const BACKTICK = "`";
const EMPTY = "";
const PIPE_OUTPUT = "pipe";
const TABLE_ROW = "tr";
const TEST_HEADING = /^Test Specification/;
const RUBRIC_HEADING = /^Acceptance (Rubric|Criteria)/;
const REQUIRED = [
  /^Overview/, /^PR Intent/, /^Implementing agent/, /^Files Changed/, /^Applicable Rules/,
  /^Applicable Gates/, /^(Prior-Art|Reference Implementation)/, /^Sections/,
  /^Interfaces/, /^Failure Modes/, /^Invariants/, /^Metrics.*Observability/,
  TEST_HEADING, RUBRIC_HEADING, /^Dead Code Sweep/, /^Out of Scope/,
  /^Product Clarity/, /^(Decomposition|Alternatives)/, /^Discovery/,
];
const RESIDUE = /<!--\s*tpl:|\{\{fill:|\{unresolved_[^}]+\}|path\/to\/file\.ext|path\/to\/spec_or_doc\.md|test_<short_name>|\{one-line reason\}|\{Slice title\}|\{smallest verifiable behaviour\}|\{one-line behavioural claim\}/;
const STAGED_MODES = new Set(["--staged", "staged"]);
const EXPECTED_COLUMN = "Expected";

function sectionsOf(text: string): Section[] {
  const sections: Section[] = [];
  const document = Bun.markdown.react(text) as MarkdownElement;
  for (const node of children(document)) {
    if (!isString(node) && node.type === "h2") sections.push({ title: textOf(node), nodes: [] });
    else if (sections.length) sections[sections.length - 1]!.nodes.push(node);
  }
  return sections;
}

function section(sections: Section[], heading: RegExp): MarkdownNode[] {
  return sections.find((entry) => heading.test(entry.title))?.nodes ?? [];
}

function children(node: MarkdownNode): MarkdownNode[] {
  return isString(node) ? [] : node.props.children ?? [];
}

function elements(nodes: MarkdownNode[], type: string): MarkdownElement[] {
  return nodes.filter((node): node is MarkdownElement => !isString(node) && node.type === type);
}

function textOf(node: MarkdownNode): string {
  if (isString(node)) return node;
  const text = children(node).map(textOf).join(EMPTY);
  return node.type === "code" ? `${BACKTICK}${text}${BACKTICK}` : text;
}

function table(nodes: MarkdownNode[]): { columns: string[]; rows: string[][] } {
  const groups = elements(nodes, "table").flatMap(children);
  const headers = elements(groups, "thead").flatMap(children);
  const bodies = elements(groups, "tbody").flatMap(children);
  return {
    columns: elements(headers, TABLE_ROW).flatMap(children).map((node) => textOf(node).trim()),
    rows: elements(bodies, TABLE_ROW).map((row) => children(row).map((node) => textOf(node).trim())),
  };
}

function testMappings(sections: Section[]): string[] {
  const errors: string[] = [];
  const items = elements(section(sections, /^Sections/), "ul").flatMap(children).map(textOf);
  const declared = items.filter((item) => item.startsWith("Dimension "));
  const dimensions = declared.flatMap((item) => [...item.matchAll(/^Dimension (\d+\.\d+)\b[^\n]*?→ Test `([^`]+)`/g)]);
  if (!dimensions.length || dimensions.length !== declared.length) errors.push("every Dimension must name its proof with → Test `name`");
  const ids = dimensions.map((match) => match[1]);
  if (new Set(ids).size !== ids.length) errors.push("Dimension identifiers must be unique");
  const tests = table(section(sections, TEST_HEADING));
  const tier = tests.columns.indexOf("Tier");
  const name = tests.columns.indexOf("Test");
  const assertion = tests.columns.findIndex((column) => column.startsWith("Asserts"));
  const id = tests.columns.indexOf("Dimension");
  if ([tier, name, assertion, id].some((index) => index < 0)) return [...errors, "Test Specification needs Dimension, Tier, Test, and Asserts columns"];
  for (const dimension of dimensions) {
    if (!tests.rows.some((row) => row[id] === dimension[1] && row[name]?.replaceAll(BACKTICK, EMPTY) === dimension[2] && row[tier] && row[assertion])) {
      errors.push(`Dimension ${dimension[1]} has no matching tiered test and assertion`);
    }
  }
  for (const row of tests.rows) {
    if (row[id] && !ids.includes(row[id])) errors.push(`test row references unknown Dimension ${row[id]}`);
  }
  return errors;
}

function readFirst(sections: Section[], exists: (path: string) => boolean): string[] {
  const items = elements(section(sections, /^Implementing agent/), "ol").flatMap(children).map(textOf);
  const references = items.flatMap((item) => [...item.matchAll(/^(?:`([^`]+)`|(https?:\/\/\S+))/g)]);
  if (!references.length) return ["read-first section needs existing repository paths or URLs"];
  const errors: string[] = [];
  for (const reference of references) {
    const path = reference[1] ?? reference[2] ?? EMPTY;
    if (/^https?:\/\//.test(path)) continue;
    const normalized = normalize(path.split("#")[0] ?? EMPTY);
    if (isAbsolute(normalized) || normalized.startsWith("..") || normalized.includes("{")) {
      errors.push(`read-first pointer must name a repository file: ${path}`);
    } else if (!exists(normalized)) {
      errors.push(`read-first pointer does not resolve: ${path}`);
    }
  }
  return errors;
}

function git(argv: string[], root: string): { ok: boolean; text: string } {
  const result = Bun.spawnSync(["git", ...argv], { cwd: root, stdout: PIPE_OUTPUT, stderr: PIPE_OUTPUT });
  return { ok: result.exitCode === 0, text: result.stdout.toString() };
}

async function commands(staged: boolean, root: string): Promise<Commands> {
  const text = staged ? git(["show", `:${CONFIG_PATH}`], root) : { ok: true, text: await Bun.file(join(root, CONFIG_PATH)).text() };
  if (!text.ok) throw new Error(`cannot read ${CONFIG_PATH} from the index`);
  const config: unknown = JSON.parse(text.text);
  if (!object(config) || !object(config.commands)) throw new Error(`${CONFIG_PATH} needs declared commands`);
  const result: Commands = {};
  for (const [name, invocations] of Object.entries(config.commands)) {
    if (!Array.isArray(invocations) || !invocations.length) throw new Error(`empty command group: ${name}`);
    result[name] = invocations.map((argv: unknown) => {
      if (!Array.isArray(argv) || !argv.length || !argv.every(isString)) throw new Error(`invalid arguments for ${name}`);
      return argv;
    });
  }
  if (!Object.keys(result).length) throw new Error(`${CONFIG_PATH} needs declared commands`);
  return result;
}

function object(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
  return typeof value === "string";
}

function rubricCommands(sections: Section[], declared: Commands): string[] {
  const rubric = table(section(sections, RUBRIC_HEADING));
  const verify = rubric.columns.findIndex((column) => column.startsWith("Verify"));
  const expected = rubric.columns.indexOf(EXPECTED_COLUMN);
  if (verify < 0 || expected < 0) return ["Acceptance Rubric needs Verify and Expected columns"];
  const errors: string[] = [];
  for (const [name, invocations] of Object.entries(declared)) {
    if (name !== "conform" && !name.startsWith("verify.")) continue;
    for (const argv of invocations) {
      const command = argv.join(" ");
      if (!rubric.rows.some((row) => row[verify] === `${BACKTICK}${command}${BACKTICK}` && row[expected])) {
        errors.push(`rubric misses declared command verbatim: ${command}`);
      }
    }
  }
  return errors;
}

export function validateSpec(text: string, declared: Commands, exists: (path: string) => boolean): string[] {
  const sections = sectionsOf(text);
  const errors: string[] = [];
  const count = text.trimEnd().split(/\r?\n/).length;
  if (count > SPEC_LINE_LIMIT) errors.push(`spec has ${count} lines; cap ${SPEC_LINE_LIMIT}`);
  for (const heading of REQUIRED) {
    if (!section(sections, heading).map(textOf).join(EMPTY).trim()) errors.push(`missing or empty required section: ${heading.source}`);
  }
  if (RESIDUE.test(text)) errors.push("unfilled template placeholder or authoring guidance");
  errors.push(...readFirst(sections, exists), ...testMappings(sections), ...rubricCommands(sections, declared));
  return errors;
}

export async function auditSpec(args: string[], root: string): Promise<string[]> {
  const [spec, path, mode] = args;
  if (!spec || !path || !mode) throw new Error("expected spec path, content path, and audit mode");
  const staged = STAGED_MODES.has(mode);
  const exists = (reference: string) => staged ? git(["cat-file", "-e", `:${reference}`], root).ok : existsSync(join(root, reference));
  return validateSpec(await Bun.file(resolve(root, path)).text(), await commands(staged, root), exists);
}

if (import.meta.main) {
  try {
    const errors = await auditSpec(Bun.argv.slice(2), process.cwd());
    for (const error of errors) {
      // logging: audit findings are the command-line interface.
      console.error(`FAIL: ${Bun.argv[2]} — ${error}`);
    }
    if (errors.length) process.exitCode = 1;
    // logging: report structural checks without claiming semantic readiness.
    else console.log(`OK: ${Bun.argv[2]} — sections, references, test mappings, and declared commands checked`);
  } catch (error) {
    // logging: unavailable audit inputs must fail, never silently skip checks.
    console.error(`FAIL: spec readiness check — ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
