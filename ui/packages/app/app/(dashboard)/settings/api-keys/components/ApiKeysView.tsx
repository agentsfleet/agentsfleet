"use client";

import { useRef } from "react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  PageHeader,
  PageLayout,
  PageTitle,
  Section,
  SectionHeader,
} from "@agentsfleet/design-system";
import type { ApiKeyListResponse } from "@/lib/api/api_keys";
import ApiKeyList, { type ApiKeyListHandle } from "./ApiKeyList";
import CreateApiKeyDialogDynamic from "@/components/domain/island-dynamic/CreateApiKeyDialogDynamic";

const API_KEYS_DESCRIPTION = "Authenticate with the agentsfleet API.";

type ApiKeysViewProps = {
  initial: ApiKeyListResponse | null;
  operatorOnly: boolean;
};

// Client wrapper so the "Create key" action and the list share a refresh
// without a full-route reload: the dialog calls the list's ref on create, which
// re-fetches just the list (page 1) via its Server Action.
export default function ApiKeysView({ initial, operatorOnly }: ApiKeysViewProps) {
  const listRef = useRef<ApiKeyListHandle>(null);
  return (
    <PageLayout>
      <PageHeader description={API_KEYS_DESCRIPTION}>
        <PageTitle>API Keys</PageTitle>
      </PageHeader>

      {operatorOnly || !initial ? (
        <Alert variant="warning">
          <div>
            <AlertTitle>API keys need admin access</AlertTitle>
            <AlertDescription>Ask a tenant admin to manage API keys.</AlertDescription>
          </div>
        </Alert>
      ) : (
        <Section asChild>
          <section aria-label="API keys">
            <SectionHeader actions={<CreateApiKeyDialogDynamic onCreated={() => listRef.current?.refresh()} />}>
              Manage API keys
            </SectionHeader>
            <ApiKeyList ref={listRef} initial={initial} />
          </section>
        </Section>
      )}
    </PageLayout>
  );
}
