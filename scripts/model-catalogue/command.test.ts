import { afterEach, describe, expect, test } from "bun:test";
import { tmpdir } from "node:os";
import { runSeedCommand } from "./command";
import { APPLY_FLAG, catalogueFixture, MODEL_NAME, modelFixture, priceFixture } from "./test-fixtures";

const FILES: string[] = [];
const SECRET_SENTINEL = `not-a-real-password-${crypto.randomUUID()}`;

async function fixture(contents = JSON.stringify(catalogueFixture())): Promise<string> {
  const path = `${tmpdir()}/agentsfleet-seeder-${crypto.randomUUID()}.json`;
  FILES.push(path);
  await Bun.write(path, contents);
  return path;
}

afterEach(async () => {
  for (const path of FILES.splice(0)) await Bun.file(path).delete();
});

describe("catalogue command", () => {
  test("offline diff reports the wanted rows without opening a database", async () => {
    const lines: unknown[][] = [];
    const status = await runSeedCommand({ args: [], databaseUrl: undefined, cataloguePath: await fixture() },
      (...args: unknown[]) => lines.push(args));
    expect(status).toBe(0);
    expect(lines.every((args) => args.length === 1)).toBe(true);
    expect(lines.flat()).toContain("1 added; 0 changed; 0 deleted");
    expect(lines.flat()).toContain("✓ diff only; no rows written");
  });

  test("applying without a database is refused with a concrete instruction", async () => {
    const lines: string[] = [];
    expect(await runSeedCommand({ args: [APPLY_FLAG], databaseUrl: undefined, cataloguePath: await fixture() },
      (line) => lines.push(line))).toBe(1);
    expect(lines.at(-1)).toBe("✗ --apply requires DATABASE_URL");
  });

  test.each([{ args: ["--unknown"] }, { args: [APPLY_FLAG, APPLY_FLAG] }])("unknown or repeated flags cannot trigger writes: %j", async ({ args }) => {
    const lines: string[] = [];
    expect(await runSeedCommand({ args, databaseUrl: undefined, cataloguePath: await fixture() },
      (line) => lines.push(line))).toBe(1);
    expect(lines).toEqual(["✗ catalogue refresh failed (validation or connection error); no credentials logged"]);
  });

  test.each([
    `https://user:${SECRET_SENTINEL}@example.com/database`,
    `postgres://user:${SECRET_SENTINEL}@127.0.0.1:1/database?sslmode=disable`,
  ])("invalid and unavailable database connections never log credentials", async (databaseUrl) => {
    const lines: string[] = [];
    expect(await runSeedCommand({ args: [], databaseUrl, cataloguePath: await fixture() },
      (line) => lines.push(line))).toBe(1);
    expect(lines.at(-1)).toStartWith("✗ catalogue refresh failed (");
    expect(lines.join("\n")).not.toContain(SECRET_SENTINEL);
    expect(lines.join("\n")).not.toContain(databaseUrl);
  });

  test("malformed or missing catalogue data is a failure and never becomes an empty apply", async () => {
    const cases = [
      { cataloguePath: await fixture("{"), code: "validation or connection error" },
      { cataloguePath: `${tmpdir()}/${crypto.randomUUID()}.json`, code: "ENOENT" },
    ];
    for (const { cataloguePath, code } of cases) {
      const lines: string[] = [];
      expect(await runSeedCommand({ args: [APPLY_FLAG], databaseUrl: undefined, cataloguePath },
        (line) => lines.push(line))).toBe(1);
      expect(lines).toEqual([`✗ catalogue refresh failed (${code}); no credentials logged`]);
    }
  });

  test("invalid catalogue entries name only curated identities and never echo invalid fields", async () => {
    const data = { ...catalogueFixture(), models: [{ ...modelFixture(),
      prices: [{ ...priceFixture(), source_url: `https://user:${SECRET_SENTINEL}@example.com/rates` }] }] };
    for (const raw of [data, null]) {
      const lines: string[] = [];
      expect(await runSeedCommand({ args: [], databaseUrl: undefined, cataloguePath: await fixture(JSON.stringify(raw)) },
        (line) => lines.push(line))).toBe(1);
      expect(lines.join("\n")).not.toContain(SECRET_SENTINEL);
      if (raw !== null) expect(lines[0]).toContain(`invalid catalogue entry ${MODEL_NAME};`);
    }
  });
});
