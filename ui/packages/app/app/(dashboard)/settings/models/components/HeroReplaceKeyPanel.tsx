"use client";

import { useState } from "react";
import { Alert, Button, DashboardPanel, Input, Spinner } from "@agentsfleet/design-system";
import { rotateSecretAction } from "../actions";
import { captureKeyRotated } from "../lib/track";
import { useProviderAction } from "../lib/use-provider-action";

type Props = {
  workspaceId: string;
  /** The active credential whose secret is rotated; provider/model are preserved. */
  secretRef: string;
  /** Active provider id — recorded on the key_rotated event (not the secret). */
  provider: string;
  /** Shown in the footer so the user knows the model is unchanged. */
  currentModel: string;
  onClose: () => void;
};

const REPLACE_KEY_ACTION = "replace the key";

/** Hero "Replace key" — PATCH-rotates only the secret; provider + model stay put. */
export default function HeroReplaceKeyPanel({
  workspaceId,
  secretRef,
  provider,
  currentModel,
  onClose,
}: Props) {
  const [key, setKey] = useState("");
  const { pending, error, run } = useProviderAction();

  function save() {
    if (key.trim() === "") return;
    void run(
      REPLACE_KEY_ACTION,
      async () => {
        const res = await rotateSecretAction(workspaceId, secretRef, key.trim());
        if (!res.ok) return { message: res.error, errorCode: res.errorCode };
        captureKeyRotated(provider);
        return null;
      },
      onClose,
    );
  }

  return (
    <DashboardPanel className="space-y-3" data-testid="hero-replace-key">
      <Input
        aria-label="New API key"
        type="password"
        value={key}
        onChange={(e) => setKey(e.target.value)}
        placeholder="sk-ant-..."
        spellCheck={false}
        autoComplete="off"
        className="font-mono"
      />
      <div className="flex flex-wrap gap-md">
        <Button type="button" onClick={save} disabled={pending || key.trim() === ""}>
          {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
          Save key
        </Button>
        <Button type="button" variant="outline" disabled={pending} onClick={onClose}>
          Cancel
        </Button>
      </div>
      <p className="font-mono text-xs text-muted-foreground">Model stays {currentModel}.</p>
      {error ? (
        <Alert variant="destructive" className="text-xs">
          {error}
        </Alert>
      ) : null}
    </DashboardPanel>
  );
}
