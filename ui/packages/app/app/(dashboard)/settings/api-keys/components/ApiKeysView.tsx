"use client";

import { useRef } from "react";
import { Section, SectionLabel } from "@agentsfleet/design-system";
import type { ApiKeyListResponse } from "@/lib/api/api_keys";
import SettingsTabs from "@/components/layout/SettingsTabs";
import ApiKeyList, { type ApiKeyListHandle } from "./ApiKeyList";
import CreateApiKeyDialogDynamic from "@/components/domain/island-dynamic/CreateApiKeyDialogDynamic";

const API_KEYS_DESCRIPTION = "Authenticate with the agentsfleet API. Each key is shown once.";

// Client wrapper so the header "New API key" action and the list share a refresh
// without a full-route reload: the dialog calls the list's ref on create, which
// re-fetches just the list (page 1) via its Server Action.
export default function ApiKeysView({ initial }: { initial: ApiKeyListResponse }) {
  const listRef = useRef<ApiKeyListHandle>(null);
  return (
    <div className="space-y-8">
      <SettingsTabs title="Workspace" description={API_KEYS_DESCRIPTION} />
      <Section asChild>
        <section aria-label="API keys">
          <div className="flex flex-wrap items-baseline justify-between gap-md">
            <SectionLabel>Manage API keys</SectionLabel>
            <CreateApiKeyDialogDynamic onCreated={() => listRef.current?.refresh()} />
          </div>
          <ApiKeyList ref={listRef} initial={initial} />
        </section>
      </Section>
    </div>
  );
}
