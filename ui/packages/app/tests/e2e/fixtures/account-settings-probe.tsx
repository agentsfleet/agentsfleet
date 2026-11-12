import type { AccountSession, AccountSessionPage, AccountSettingsClient } from "@/lib/auth/account-types";
import type { ActionResult } from "@/lib/actions/with-token";
import { useState } from "react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { Button, TooltipProvider } from "@agentsfleet/design-system";
import { ShellFrame } from "@/components/layout/ShellFrame";
import ApiKeysView from "@/app/(dashboard)/settings/security/components/ApiKeysView";
import { SecurityLayout } from "@/app/(dashboard)/settings/security/components/SecurityLayout";
import { SessionSettingsView } from "@/components/account/SessionSettings";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { WORKSPACE } from "@/tests/helpers/draft-fixtures";

const SESSIONS: AccountSession[] = [
  {id: "current", browser: "Chrome", device: "macOS", current: true, mobile: false, location: "Chennai, IN", lastActiveAt: "2026-11-11T16:00:00Z"},
  {id: "phone", browser: "Safari", device: "iPhone", current: false, mobile: true, location: null, lastActiveAt: null},
];
const router = {bfcacheId: "account-review", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};

/** Real settings views and shell, with explicitly isolated sample data. */
export function AccountSettingsFixture() {


  const [failed, setFailed] = useState(false);
  const [client] = useState(() => new AccountFixtureClient());
  return <AppRouterContext value={router}><PathnameContext.Provider value="/settings/security">
    <SearchParamsContext.Provider value={new URLSearchParams()}><TooltipProvider>
      <style>{".app-shell{top:46px;height:calc(100dvh - 46px)}"}</style>
      <div className="fixed inset-x-0 top-0 z-50 flex h-11.5 items-center gap-sm overflow-x-auto whitespace-nowrap bg-surface-deep px-lg">
        <span className="mr-auto text-eyebrow text-muted-foreground">Component review · Sample account</span>
        <Button size="sm" variant="ghost" aria-pressed={failed} onClick={() => {client.setUnavailable(!failed); setFailed(!failed);}}>Errors</Button>
        <Button size="sm" variant="ghost" onClick={() => {document.documentElement.dataset.theme = document.documentElement.dataset.theme === "light" ? "dark" : "light";}}>Theme</Button>
      </div>
      <ShellFrame workspaces={[{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}]}>
        <SecurityLayout>
          <ApiKeysView operatorOnly={false} initial={{items: [{id: "sample-key", key_name: "Deploy automation", active: true, created_at: "2026-11-04T10:00:00Z", revoked_at: null}], total: 1, next_cursor: null}} />
          <SessionSettingsView key={String(failed)} client={client} />
        </SecurityLayout>
      </ShellFrame>
    </TooltipProvider></SearchParamsContext.Provider>
  </PathnameContext.Provider></AppRouterContext>;
}

class AccountFixtureClient implements AccountSettingsClient {
  #unavailable = false;
  #records = [...SESSIONS];
  setUnavailable(value: boolean) {this.#unavailable = value;}
  async listSessions(): Promise<ActionResult<AccountSessionPage>> {
    return this.#unavailable ? {ok: false, error: "Sessions could not be loaded. Try again."}
      : {ok: true, data: {sessions: this.#records, nextOffset: null}};
  }
  async revokeSession(id: string): Promise<ActionResult<{id: string}>> {
    if (this.#unavailable) return {ok: false, error: "Sign-out is unconfirmed. Refresh before trying again."};
    this.#records = this.#records.filter(session => session.id !== id);
    return {ok: true, data: {id}};
  }
}
