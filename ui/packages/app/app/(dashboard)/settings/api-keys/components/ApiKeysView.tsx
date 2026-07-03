"use client";

import { useRef } from "react";
import { Section } from "@agentsfleet/design-system";
import type { ApiKeyListResponse } from "@/lib/api/api_keys";
import SettingsTabs from "@/components/layout/SettingsTabs";
import ApiKeyList, { type ApiKeyListHandle } from "./ApiKeyList";
import CreateApiKeyDialogDynamic from "@/components/domain/island-dynamic/CreateApiKeyDialogDynamic";

// Client wrapper so the header "New API key" action and the list share a refresh
// without a full-route reload: the dialog calls the list's ref on create, which
// re-fetches just the list (page 1) via its Server Action.
export default function ApiKeysView({ initial }: { initial: ApiKeyListResponse }) {
  const listRef = useRef<ApiKeyListHandle>(null);
  return (
    <div className="space-y-8">
      <SettingsTabs title="Workspace" />
      <div className="flex items-start justify-between gap-4">
        <p className="max-w-2xl text-sm text-muted-foreground">
          Keys let outside tools call agentsfleet on behalf of this workspace. Each key is shown once.
        </p>
        <CreateApiKeyDialogDynamic onCreated={() => listRef.current?.refresh()} />
      </div>
      <Section asChild>
        <section aria-label="API keys">
          <ApiKeyList ref={listRef} initial={initial} />
        </section>
      </Section>
    </div>
  );
}
