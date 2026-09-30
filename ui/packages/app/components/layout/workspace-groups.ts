import { ACCOUNT_ROLE, type TenantWorkspace } from "@/lib/api/workspaces";

// The switcher's menu, cut into sections by account. A person holding only
// their own account sees one unlabelled section, exactly the list they saw
// before accounts could be shared. Once they hold another account, their own
// comes first under "Yours" and each joined account follows under its owner's
// name, in the order the list first names it.

export const OWN_ACCOUNT_LABEL = "Yours";

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
  /** Null for the one unlabelled section a single account renders as. */
  label: string | null;
  workspaces: SwitcherWorkspace[];
};

export function accountLabel(ownerName: string): string {
  return `${ownerName}'s account`;
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
    const section = joined.get(workspace.account.tenant_id);
    if (section) {
      section.workspaces.push(entry);
    } else {
      joined.set(workspace.account.tenant_id, {
        label: accountLabel(workspace.account.owner_name),
        workspaces: [entry],
      });
    }
  }
  own.push(...created.filter((fresh) => !listed.some((workspace) => workspace.id === fresh.id)));

  const lead: SwitcherSection[] = routed ? [{ label: null, workspaces: [routed] }] : [];
  if (joined.size === 0) {
    return [...lead, { label: null, workspaces: own }];
  }
  const ownSection: SwitcherSection[] =
    own.length > 0 ? [{ label: OWN_ACCOUNT_LABEL, workspaces: own }] : [];
  return [...lead, ...ownSection, ...joined.values()];
}
