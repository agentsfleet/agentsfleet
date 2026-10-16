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

const API_KEYS_DESCRIPTION = "Let your scripts and tools work with agentsfleet. Create a key for each connection.";

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
            <AlertDescription>Ask your account owner to give you access to API keys.</AlertDescription>
          </div>
        </Alert>
      ) : (
        <Section aria-label="Account API keys">
            <SectionHeader as="p" actions={<CreateApiKeyDialogDynamic onCreated={() => listRef.current?.refresh()} />}>Account keys</SectionHeader>
            <ApiKeyList ref={listRef} initial={initial} />
            <p className="text-label text-muted-foreground">Keys belong to your account. Keep them private and revoke any you no longer use.</p>
        </Section>
      )}
    </PageLayout>
  );
}
