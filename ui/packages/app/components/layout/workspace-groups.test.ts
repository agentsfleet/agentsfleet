import { describe, expect, it } from "vitest";
import { ACCOUNT_ROLE, type TenantWorkspace } from "@/lib/api/workspaces";
import { accountLabel, OWN_ACCOUNT_LABEL, switcherSections } from "./workspace-groups";

const OWN = { tenant_id: "tenant_me", owner_name: "Me" };
const JOHN = { tenant_id: "tenant_john", owner_name: "John" };
const MARY = { tenant_id: "tenant_mary", owner_name: "Mary" };
// A second account whose owner is also called John.
const OTHER_JOHN = { tenant_id: "tenant_john_2", owner_name: JOHN.owner_name };

function own(id: string): TenantWorkspace {
  return { id, name: id, created_at: 1, account: OWN, role: ACCOUNT_ROLE.owner };
}

function joined(id: string, account: typeof JOHN): TenantWorkspace {
  return { id, name: id, created_at: 1, account, role: ACCOUNT_ROLE.member };
}

describe("switcherSections", () => {
  it("should render a solo account as one unlabelled section, exactly as before accounts could be shared", () => {
    expect(switcherSections([own("a"), own("b")], [], null)).toMatchObject([
      { label: null, workspaces: [{ id: "a", name: "a" }, { id: "b", name: "b" }] },
    ]);
  });

  it("should put the caller's own account first under Yours once another account is held", () => {
    const sections = switcherSections([joined("j1", JOHN), own("a")], [], null);
    expect(sections.map((section) => section.label)).toEqual([OWN_ACCOUNT_LABEL, accountLabel(JOHN.owner_name)]);
    expect(sections[0]?.workspaces.map((w) => w.id)).toEqual(["a"]);
  });

  it("should keep one account's workspaces together and accounts in the order the list first names them", () => {
    const sections = switcherSections(
      [joined("m1", MARY), joined("j1", JOHN), joined("m2", MARY), own("a")],
      [],
      null,
    );
    expect(sections.map((section) => [section.label, section.workspaces.map((w) => w.id)])).toEqual([
      [OWN_ACCOUNT_LABEL, ["a"]],
      [accountLabel(MARY.owner_name), ["m1", "m2"]],
      [accountLabel(JOHN.owner_name), ["j1"]],
    ]);
  });

  it("should omit the Yours section when every listed workspace belongs to a joined account", () => {
    const sections = switcherSections([joined("j1", JOHN)], [], null);
    expect(sections.map((section) => section.label)).toEqual([accountLabel(JOHN.owner_name)]);
  });

  it("should file a just-created workspace under the caller's own account, once", () => {
    const sections = switcherSections(
      [own("a"), joined("j1", JOHN)],
      [{ id: "fresh", name: "fresh" }, { id: "a", name: "stale copy" }],
      null,
    );
    expect(sections[0]).toMatchObject({
      label: OWN_ACCOUNT_LABEL,
      workspaces: [{ id: "a", name: "a" }, { id: "fresh", name: "fresh" }],
    });
  });

  it("should lead with an unplaceable routed workspace, outside every account", () => {
    const routed = { id: "unknown", name: "Current workspace" };
    const grouped = switcherSections([own("a"), joined("j1", JOHN)], [], routed);
    expect(grouped[0]).toMatchObject({ label: null, workspaces: [routed] });
    const solo = switcherSections([own("a")], [], routed);
    expect(solo).toMatchObject([
      { label: null, workspaces: [routed] },
      { label: null, workspaces: [{ id: "a", name: "a" }] },
    ]);
  });

  it("should return one empty unlabelled section for a person with no workspaces", () => {
    expect(switcherSections([], [], null)).toMatchObject([{ label: null, workspaces: [] }]);
  });

  it("should key a joined account by its tenant, so two owners with one name stay two sections", () => {
    const routed = { id: "unknown", name: "Current workspace" };
    const sections = switcherSections([own("a"), joined("j1", JOHN), joined("j2", OTHER_JOHN)], [], routed);
    const keys = sections.map((section) => section.key);
    expect(keys.slice(-2)).toEqual([JOHN.tenant_id, OTHER_JOHN.tenant_id]);
    expect(new Set(keys).size).toBe(keys.length);
    expect(sections.slice(-2).map((section) => section.label)).toEqual([
      accountLabel(JOHN.owner_name),
      accountLabel(OTHER_JOHN.owner_name),
    ]);
  });
});

describe("accountLabel", () => {
  it("should name an account by its owner", () => {
    // pin test: literal is the contract — the wording the invite email uses too.
    expect(accountLabel("John")).toBe("John's account");
  });
});
