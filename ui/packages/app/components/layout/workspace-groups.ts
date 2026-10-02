import type { TenantWorkspace } from "@/lib/api/workspaces";
import { ACCOUNT_ROLE } from "@/lib/api/workspaces-types";

// The switcher's menu, cut into sections by account. A person holding only
// their own account sees one unlabelled section. Once they hold another
// account, their own comes first under "Yours" and each joined account follows
// under its owner's name, in the order the list first names it.

export const OWN_ACCOUNT_LABEL = "Yours";

/** What follows an owner's name to name their account. Mirrors
 * `ACCOUNT_LABEL_SUFFIX` in `rustd/crates/afd_mail/src/invite.rs`, so the
 * invite email and the dashboard name an account the same way. */
export const ACCOUNT_LABEL_SUFFIX = "'s account";

// Keys for the two sections no joined account owns. A joined account's section
// is keyed by its tenant id, since two owners can share a name.
const OWN_SECTION_KEY = "own";
const ROUTED_SECTION_KEY = "routed";

/** What the switcher calls a workspace with no name, one it cannot place, and none. */
export const WORKSPACE_LABEL = {
  unnamed: "Unnamed workspace",
  current: "Current workspace",
  none: "No workspace",
} as const;

/** What the menu needs to render and pick one workspace. */
export type SwitcherWorkspace = {
  id: string;
  name: string | null;
};

export type SwitcherSection = {
  /** Unique within the menu: a joined account's tenant id, or a fixed key. */
  key: string;
  /** Null for the one unlabelled section a single account renders as. */
  label: string | null;
  workspaces: SwitcherWorkspace[];
};

export function accountLabel(ownerName: string): string {
  return `${ownerName}${ACCOUNT_LABEL_SUFFIX}`;
}

/**
 * The menu's sections. `created` are workspaces made in this session that the
 * server list has not caught up with — always the caller's own. `routed` is the
 * placeholder for a routed workspace neither source knows; it leads the menu,
 * outside any account, because nothing says whose it is.
 */
export function switcherSections(
  listed: readonly TenantWorkspace[],
  created: readonly SwitcherWorkspace[],
  routed: SwitcherWorkspace | null,
): SwitcherSection[] {
  const own: SwitcherWorkspace[] = [];
  const joined = new Map<string, SwitcherSection>();
  for (const workspace of listed) {
    const entry = { id: workspace.id, name: workspace.name };
    if (workspace.role === ACCOUNT_ROLE.owner) {
      own.push(entry);
      continue;
    }
    const tenantId = workspace.account.tenant_id;
    const section = joined.get(tenantId);
    if (section) {
      section.workspaces.push(entry);
    } else {
      joined.set(tenantId, {
        key: tenantId,
        label: accountLabel(workspace.account.owner_name),
        workspaces: [entry],
      });
    }
  }
  own.push(...created.filter((fresh) => !listed.some((workspace) => workspace.id === fresh.id)));

  const lead: SwitcherSection[] = routed ? [{ key: ROUTED_SECTION_KEY, label: null, workspaces: [routed] }] : [];
  if (joined.size === 0) {
    return [...lead, { key: OWN_SECTION_KEY, label: null, workspaces: own }];
  }
  const ownSection: SwitcherSection[] =
    own.length > 0 ? [{ key: OWN_SECTION_KEY, label: OWN_ACCOUNT_LABEL, workspaces: own }] : [];
  return [...lead, ...ownSection, ...joined.values()];
}
