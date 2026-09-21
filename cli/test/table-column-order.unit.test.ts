// One table shape. The helper owns the order, so a call site has no position
// to place and cannot omit the age column by being edited.

import { describe, expect, test } from "bun:test";

import {
  entityColumns,
  entityTable,
  EMPTY_CELL,
  type EntityTableSpec,
} from "../src/output/format.ts";

const NAME = { key: "name", label: "NAME" } as const;
const IDENTIFIER = { key: "id", label: "FLEET" } as const;
const STATUS = { key: "status", label: "STATUS" } as const;
const WIDE = { widthHint: 200 } as const;

const labels = (spec: EntityTableSpec): string[] =>
  entityColumns(spec).map((column) => column.label);

describe("entityColumns fixes one order", () => {
  test("name, then identifier, then domain columns, then the age", () => {
    expect(labels({ name: NAME, id: IDENTIFIER, domain: [STATUS] }))
      .toEqual(["NAME", "FLEET", "STATUS", "AGO"]);
  });

  test("a table with no identifier still ends with the age", () => {
    expect(labels({ name: NAME, domain: [STATUS] }))
      .toEqual(["NAME", "STATUS", "AGO"]);
  });

  test("a table with no domain columns is name, identifier, age", () => {
    expect(labels({ name: NAME, id: IDENTIFIER, domain: [] }))
      .toEqual(["NAME", "FLEET", "AGO"]);
  });

  test("an entity with no name of its own leads with its identifier", () => {
    // A hosted schedule has no name — its identity IS the identifier, and
    // inventing one would be a column that says nothing.
    expect(labels({ id: IDENTIFIER, domain: [STATUS] }))
      .toEqual(["FLEET", "STATUS", "AGO"]);
  });

  test("domain order is the caller's, and it is preserved exactly", () => {
    const tier = { key: "tier", label: "TIER" } as const;
    const secrets = { key: "credentials", label: "SECRETS" } as const;
    expect(labels({ name: NAME, id: IDENTIFIER, domain: [tier, secrets] }))
      .toEqual(["NAME", "FLEET", "TIER", "SECRETS", "AGO"]);
  });
});

describe("a domain column cannot claim the age column", () => {
  test("by its key", () => {
    expect(() => entityColumns({
      name: NAME, domain: [{ key: "created_at", label: "CREATED" }],
    })).toThrow("appended by entityColumns");
  });

  test("by its label", () => {
    expect(() => entityColumns({
      name: NAME, domain: [{ key: "whenever", label: "AGO" }],
    })).toThrow("appended by entityColumns");
  });
});

describe("entityTable renders the age from the row", () => {
  // entityTable renders through the real clock, so the fixture is anchored to it.
  const now = Date.now();
  const spec: EntityTableSpec = { name: NAME, id: IDENTIFIER, domain: [STATUS] };

  test("a row carrying a timestamp shows its age under AGO", () => {
    const rendered = entityTable(
      spec,
      [{ name: "reviewer", id: "abc", status: "active", created_at: now - 7_200_000 }],
      WIDE,
    );
    expect(rendered).toContain("AGO");
    expect(rendered).toContain("2h");
  });

  test("a table whose rows carry no timestamp still renders the column", () => {
    const rendered = entityTable(
      spec, [{ name: "reviewer", id: "abc", status: "active" }], WIDE,
    );
    expect(rendered).toContain("AGO");
    expect(rendered).toContain(EMPTY_CELL);
  });

  test("a table ages by the field it names, not only by created_at", () => {
    const rendered = entityTable(
      { name: NAME, domain: [STATUS], ageKey: "updated_at" },
      [{ name: "note", status: "kept", updated_at: now - 7_200_000 }],
      WIDE,
    );
    expect(rendered).toContain("AGO");
    expect(rendered).toContain("2h");
  });

  test("a row's own created_at value never reaches the output raw", () => {
    const rendered = entityTable(
      spec,
      [{ name: "reviewer", id: "abc", status: "active", created_at: now }],
      WIDE,
    );
    expect(rendered).not.toContain(String(now));
  });
});
