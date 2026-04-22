"use client";

import { useState } from "react";
import { CheckIcon, CopyIcon } from "lucide-react";
import { buttonClassName } from "@usezombie/design-system";
import { webhookUrlFor } from "@/lib/api/zombies";

type Props = { zombieId: string };

export default function TriggerPanel({ zombieId }: Props) {
  const [mode, setMode] = useState<"webhook" | "schedule">("webhook");
  const [copied, setCopied] = useState(false);
  const url = webhookUrlFor(zombieId);

  async function onCopy() {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard API unavailable — no-op, user can select the text
    }
  }

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div role="tablist" className="mb-4 flex gap-2">
        <button
          role="tab"
          aria-selected={mode === "webhook"}
          onClick={() => setMode("webhook")}
          className={buttonClassName(
            mode === "webhook" ? "default" : "ghost",
            "sm",
          )}
        >
          Webhook (event-driven)
        </button>
        <button
          role="tab"
          aria-selected={mode === "schedule"}
          onClick={() => setMode("schedule")}
          className={buttonClassName(
            mode === "schedule" ? "default" : "ghost",
            "sm",
          )}
        >
          Schedule (cron)
        </button>
      </div>

      {mode === "webhook" ? (
        <div>
          <label className="mb-1 block text-xs uppercase tracking-wide text-muted-foreground">
            Webhook URL
          </label>
          <div className="flex items-center gap-2">
            <code
              data-testid="webhook-url"
              className="flex-1 rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs break-all"
            >
              {url}
            </code>
            <button
              type="button"
              onClick={onCopy}
              className={buttonClassName("ghost", "sm")}
              aria-label="Copy webhook URL"
            >
              {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <p className="mt-3 text-xs text-muted-foreground">
            Paste this URL into AgentMail, Grafana, Slack Events API, or any
            webhook-capable service.
          </p>
        </div>
      ) : (
        <div className="rounded-md border border-dashed border-border bg-muted/20 px-4 py-6 text-sm text-muted-foreground">
          <p className="font-medium text-foreground">
            Cron scheduling is CLI-only for V1.
          </p>
          <p className="mt-1">
            Use{" "}
            <code className="font-mono text-xs">
              zombiectl zombie schedule
            </code>{" "}
            for now. A UI editor ships once the backend schedule endpoint
            is available.
          </p>
        </div>
      )}
    </div>
  );
}
