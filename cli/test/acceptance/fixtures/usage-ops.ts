/**
 * usage-ops.ts — `usage show`, paged the way a terminal user pages it.
 *
 * The figures come from `--json`; what a person reads comes from the text
 * view, whose only paging control is the `--cursor` hint printed under it.
 * Both walks start at the newest charge, and the lane's other live files bill
 * the same tenant meanwhile, so a run's charge is looked for across pages
 * rather than expected on the first one.
 */

import type { RunResult } from "./cli.js";
import assert from "node:assert/strict";
import { Schema } from "effect";

import { instant } from "../../../src/lib/instant.ts";

/** A CLI runner that already applies the calling spec's leak checks. */
export type Run = (args: ReadonlyArray<string>) => Promise<RunResult>;

const USAGE_SHOW = ["usage", "show"] as const;
const JSON_FLAG = "--json" as const;
const LIMIT_FLAG = "--limit" as const;
const CURSOR_FLAG = "--cursor" as const;
// The CLI's own ceiling on `--limit` (`MAX_LIMIT`, src/commands/billing/billing.ts).
const PAGE_ROWS = 100;
// A thousand ledger rows newer than the run under test is far past what the
// lane's other files bill while one test runs.
const MAX_PAGES = 10;
// The text view's continuation line: `(more costs available — re-run with --cursor <token>)`.
const NEXT_PAGE_HINT = /re-run with --cursor (\S+)\)/u;

const UsageWorkSchema = Schema.Struct({
  fleet_id: Schema.optionalKey(Schema.NullOr(Schema.String)),
  turn_id: Schema.optionalKey(Schema.NullOr(Schema.String)),
  recorded_at: Schema.NullOr(instant),
  total_nanos: Schema.String,
});
const UsagePageSchema = Schema.Struct({
  events: Schema.Array(UsageWorkSchema),
  next_cursor: Schema.NullOr(Schema.String),
});

export type UsagePage = typeof UsagePageSchema.Type;

const cursorArgs = (cursor: string | undefined): ReadonlyArray<string> =>
  cursor === undefined ? [] : [CURSOR_FLAG, cursor];

/** One `usage show --json` page, at most `limit` ledger rows grouped by work. */
export async function readUsagePage(run: Run, limit: number, cursor?: string): Promise<UsagePage> {
  const result = await run([...USAGE_SHOW, LIMIT_FLAG, String(limit), ...cursorArgs(cursor), JSON_FLAG]);
  assert.equal(result.code, 0, `usage show --json exited ${result.code}: ${result.stderr}`);
  return Schema.decodeUnknownSync(UsagePageSchema)(JSON.parse(result.stdout.trim()));
}

/**
 * What `usage show --json` charges `fleetId` for `turnId`, in nanos.
 *
 * A run writes several ledger rows and a page boundary can fall between them,
 * so the walk adds up every page the work appears on and stops at the first
 * page after them that holds none.
 */
export async function chargedNanos(run: Run, fleetId: string, turnId: string): Promise<bigint> {
  let cursor: string | undefined;
  let total = 0n;
  let seen = false;
  for (let page = 0; page < MAX_PAGES; page += 1) {
    const read = await readUsagePage(run, PAGE_ROWS, cursor);
    const mine = read.events.filter((event) => event.fleet_id === fleetId && event.turn_id === turnId);
    if (seen && mine.length === 0) break;
    for (const event of mine) total += BigInt(event.total_nanos);
    seen ||= mine.length > 0;
    if (read.next_cursor === null) break;
    cursor = read.next_cursor;
  }
  assert.ok(seen, `usage show --json never listed fleet ${fleetId}'s charge for work ${turnId}`);
  return total;
}

/** The text page that names `workId`, reached by following the printed `--cursor` hint. */
export async function usageTextNaming(run: Run, workId: string): Promise<string> {
  let cursor: string | undefined;
  for (let page = 0; page < MAX_PAGES; page += 1) {
    const result = await run([...USAGE_SHOW, LIMIT_FLAG, String(PAGE_ROWS), ...cursorArgs(cursor)]);
    assert.equal(result.code, 0, `usage show exited ${result.code}: ${result.stderr}`);
    if (result.stdout.includes(workId)) return result.stdout;
    const next = NEXT_PAGE_HINT.exec(result.stdout)?.[1];
    if (next === undefined) break;
    cursor = next;
  }
  throw new assert.AssertionError({ message: `usage show never printed a row for work ${workId}` });
}
