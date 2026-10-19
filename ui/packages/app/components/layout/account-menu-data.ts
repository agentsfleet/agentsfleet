import { ActivityIcon, BookOpenIcon, BrainCircuitIcon, CircleCheckIcon, CreditCardIcon, KeyRoundIcon, LockKeyholeIcon, LibraryIcon, PlugIcon, ServerIcon, UsersIcon, UserIcon } from "lucide-react";
import { SCOPE } from "@/lib/auth/scopes";
import type { NavEntry, PlatformNavEntry } from "./sidebar-navigation-data";

export const WORKSPACE_NAV: NavEntry[] = [
  { label: "Activity", path: "events", icon: ActivityIcon, workspaceScoped: true },
  { label: "Approvals", path: "approvals", icon: CircleCheckIcon, workspaceScoped: true },
  { label: "Models", path: "settings/models", icon: BrainCircuitIcon, workspaceScoped: true },
  { label: "Integrations", path: "integrations", icon: PlugIcon, workspaceScoped: true },
  { label: "Secrets", path: "secrets", icon: LockKeyholeIcon, workspaceScoped: true },
  { label: "Fleet library", path: "library", icon: LibraryIcon, workspaceScoped: true },
];

export const PLATFORM_NAV: PlatformNavEntry[] = [
  { label: "Runners", path: "/admin/runners", icon: ServerIcon, scope: SCOPE.RUNNER_READ },
  { label: "Model library", path: "/admin/models", icon: BrainCircuitIcon, scope: SCOPE.MODEL_READ },
  { label: "Fleet library", path: "/admin/fleet-libraries", icon: LibraryIcon, scope: SCOPE.PLATFORM_LIBRARY_WRITE },
];

// Each person manages the account signup created for them.
export const ACCOUNT_NAV: NavEntry[] = [
  { label: "Profile", path: "/settings/account", icon: UserIcon },
  { label: "Team", path: "/settings/members", icon: UsersIcon },
  { label: "API keys", path: "/settings/api-keys", icon: KeyRoundIcon },
  { label: "Billing", path: "/settings/billing", icon: CreditCardIcon },
];

export const ACCOUNT_SHORTCUTS: NavEntry[] = [
  { label: "Usage", path: "/settings/billing", icon: CreditCardIcon },
];

export const SERVICE_SHORTCUTS: NavEntry[] = [
  { label: "Connect apps", path: "integrations", icon: PlugIcon, workspaceScoped: true },
  { label: "Get API keys", path: "/settings/api-keys", icon: KeyRoundIcon },
  { label: "Manage secrets", path: "secrets", icon: LockKeyholeIcon, workspaceScoped: true },
];

export const BOTTOM_NAV: NavEntry[] = [
  { label: "Learn more", path: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true, analyticsSource: "app_sidebar_docs" },
];
