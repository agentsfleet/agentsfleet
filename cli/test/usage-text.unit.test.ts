import { expect, mock, test } from "bun:test";

import { ColorMode } from "../src/output/capability.ts";
import { formatTable } from "../src/output/format.ts";
import { readUsageTextTotal } from "./acceptance/fixtures/usage-ops.ts";

const WORK_ID = "018f1040-0000-7000-8000-000000000001";
const OTHER_WORK_ID = "018f1040-0000-7000-8000-000000000002";
const TOTAL = "$0.0195";
const COLUMNS = [
  { key: "id", label: "ID" },
  { key: "stage", label: "STAGE" },
  { key: "total", label: "TOTAL" },
];
const ROWS = [
  { id: OTHER_WORK_ID, stage: "$0.02", total: "$0.03" },
  { id: WORK_ID, stage: "$0.0123", total: TOTAL },
];
const WIDTHS = [
  { label: "a wide table", widthHint: 140 },
  { label: "a narrow terminal", widthHint: 40 },
];

const success = (stdout: string) => ({ code: 0, stdout, stderr: "", durationMs: 0 });

for (const { label, widthHint } of WIDTHS) {
  const opts = { mode: ColorMode.NONE, widthHint };

  test(`reads only the requested work total in ${label}`, async () => {
    const run = async () => success(formatTable(COLUMNS, ROWS, opts));
    await expect(readUsageTextTotal(run, WORK_ID)).resolves.toBe(TOTAL);
  });

  test(`rejects the requested work without a total in ${label}`, async () => {
    const run = async () => success(formatTable(COLUMNS.slice(0, -1), ROWS, opts));
    await expect(readUsageTextTotal(run, WORK_ID)).rejects.toMatchObject({
      name: "AssertionError", code: "ERR_ASSERTION", message: `usage show printed no TOTAL for work ${WORK_ID}`,
    });
  });
}

test("rejects a truncated row with a dollar amount in the preceding column", async () => {
  const text = formatTable(COLUMNS, ROWS, { mode: ColorMode.NONE, widthHint: 140 });
  const run = async () => success(text.replace(TOTAL, ""));
  await expect(readUsageTextTotal(run, WORK_ID)).rejects.toMatchObject({
    name: "AssertionError", code: "ERR_ASSERTION", message: `usage show printed no TOTAL for work ${WORK_ID}`,
  });
});

test("follows the printed cursor when the work is mentioned without a row", async () => {
  const opts = { mode: ColorMode.NONE, widthHint: 40 };
  const cursor = "older-page";
  const run = mock(async (args: ReadonlyArray<string>) => success(args.includes(cursor)
    ? formatTable(COLUMNS, ROWS, opts)
    : `No row yet for ${WORK_ID}\n\n(more costs available — re-run with --cursor ${cursor})\n`));
  await expect(readUsageTextTotal(run, WORK_ID)).resolves.toBe(TOTAL);
  expect(run.mock.calls).toEqual([
    [["usage", "show", "--limit", "100"]],
    [["usage", "show", "--limit", "100", "--cursor", cursor]],
  ]);
});

test("rejects an empty activity page", async () => {
  const run = async () => success("No billable activity recorded yet.\n");
  await expect(readUsageTextTotal(run, WORK_ID)).rejects.toMatchObject({
    name: "AssertionError", code: "ERR_ASSERTION", message: `usage show never printed a row for work ${WORK_ID}`,
  });
});
