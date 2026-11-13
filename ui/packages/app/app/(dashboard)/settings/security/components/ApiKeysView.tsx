"use client";

import { useRef } from "react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Section,
  SectionHeader,
} from "@agentsfleet/design-system";
import type { ApiKeyListResponse } from "@/lib/api/api_keys";
import ApiKeyList, { type ApiKeyListHandle } from "./ApiKeyList";
import CreateApiKeyDialogDynamic from "@/components/domain/island-dynamic/CreateApiKeyDialogDynamic";


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
    <>
      {operatorOnly || !initial ? (
        <Alert variant="warning">
          <div>
            <AlertTitle>API keys need admin access</AlertTitle>
            <AlertDescription>Ask your account owner to give you access to API keys.</AlertDescription>
          </div>
        </Alert>
      ) : (
        <Section aria-label="Account API keys">
            <SectionHeader description="Use a key to connect your tools to agentsfleet." actions={<CreateApiKeyDialogDynamic onCreated={() => listRef.current?.refresh()} />}>API keys</SectionHeader>
            <ApiKeyList ref={listRef} initial={initial} />
            <p className="text-eyebrow leading-eyebrow text-text-dim">A new key is shown once. Give each tool its own key so you can revoke access independently.</p>
        </Section>
      )}
    </>
  );
}
