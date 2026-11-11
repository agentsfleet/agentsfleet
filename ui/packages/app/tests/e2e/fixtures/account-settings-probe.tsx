import type { AccountProfile, AccountSession, AccountSessionPage, AccountSettingsClient, ProfileNames } from "@/lib/auth/account-types";
import type { ActionResult } from "@/lib/actions/with-token";
import { useState } from "react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import { ShellFrame } from "@/components/layout/ShellFrame";
import { ProfileSettingsView } from "@/components/account/ProfileSettings";
import { SessionSettingsView } from "@/components/account/SessionSettings";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { WORKSPACE } from "@/tests/helpers/draft-fixtures";

const PROFILE: AccountProfile = {subject: "fixture-owner", firstName: "Kishore", lastName: "Neelamegam", email: "you@example.test", imageUrl: null};
const SESSIONS: AccountSession[] = [
  {id: "current", browser: "Chrome", device: "macOS", current: true, mobile: false, location: "Chennai, IN", lastActiveAt: "2026-11-11T16:00:00Z"},
  {id: "phone", browser: "Safari", device: "iPhone", current: false, mobile: true, location: null, lastActiveAt: null},
];
const router = {bfcacheId: "account-review", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};

/** Real settings views and shell, with explicitly isolated sample data. */
export function AccountSettingsFixture() {
  const [view, setView] = useState("security");
  const [profile, setProfile] = useState(PROFILE);
  const [failed, setFailed] = useState(false);
  const [client] = useState(() => new AccountFixtureClient(names => setProfile(previous => ({...previous, ...names})), () => setView("profile")));
  return <AppRouterContext value={router}><PathnameContext.Provider value={view === "profile" ? "/settings/account" : "/settings/security"}>
    <SearchParamsContext.Provider value={new URLSearchParams()}><TooltipProvider>
      <style>{".app-shell{top:44px;height:calc(100dvh - 44px)}"}</style>
      <div className="fixed inset-x-0 top-0 z-50 flex h-11 items-center gap-sm overflow-x-auto whitespace-nowrap bg-surface-deep px-lg">
        <span className="mr-auto text-eyebrow text-muted-foreground">Component review · Sample account</span>
        <Button size="sm" variant="ghost" onClick={() => setView("profile")}>Profile</Button>
        <Button size="sm" variant="ghost" onClick={() => setView("security")}>Security</Button>
        <Button size="sm" variant="ghost" aria-pressed={failed} onClick={() => {client.setUnavailable(!failed); setFailed(!failed);}}>Errors</Button>
        <Button size="sm" variant="ghost" onClick={() => {document.documentElement.dataset.theme = document.documentElement.dataset.theme === "light" ? "dark" : "light";}}>Theme</Button>
      </div>
      <ShellFrame workspaces={[{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}]}>
        {view === "profile" ? <ProfileSettingsView profile={profile} client={client} /> : <SessionSettingsView client={client} />}
      </ShellFrame>
    </TooltipProvider></SearchParamsContext.Provider>
  </PathnameContext.Provider></AppRouterContext>;
}

class AccountFixtureClient implements AccountSettingsClient {
  #unavailable = false;
  #records = [...SESSIONS];
  constructor(readonly save: (names: ProfileNames) => void, readonly showProfile: () => void) {}
  setUnavailable(value: boolean) {this.#unavailable = value;}
  async updateProfile(names: ProfileNames): Promise<ActionResult<null>> {
    if (this.#unavailable) return {ok: false, error: "Profile save unavailable. Your edited names are retained."};
    this.save(names);
    return {ok: true, data: null};
  }
  async listSessions(): Promise<ActionResult<AccountSessionPage>> {
    return this.#unavailable ? {ok: false, error: "Sessions could not be refreshed. Previously loaded devices are retained."}
      : {ok: true, data: {sessions: this.#records, nextOffset: null}};
  }
  async revokeSession(id: string): Promise<ActionResult<{id: string}>> {
    if (this.#unavailable) return {ok: false, error: "Sign-out is unconfirmed. Refresh before trying again."};
    this.#records = this.#records.filter(session => session.id !== id);
    return {ok: true, data: {id}};
  }
  manageSignIn = () => {this.showProfile();};
}
