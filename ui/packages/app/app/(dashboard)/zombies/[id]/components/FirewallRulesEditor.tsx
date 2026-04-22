import { ShieldIcon } from "lucide-react";

export default function FirewallRulesEditor() {
  return (
    <div className="rounded-lg border border-dashed border-border bg-muted/20 px-4 py-6 text-sm">
      <div className="flex items-start gap-3">
        <ShieldIcon
          size={18}
          className="mt-0.5 shrink-0 text-muted-foreground"
          aria-hidden="true"
        />
        <div>
          <p className="font-medium text-foreground">
            Firewall editing is CLI-only for V1.
          </p>
          <p className="mt-1 text-muted-foreground">
            Manage rules with{" "}
            <code className="font-mono text-xs">
              zombiectl zombie firewall set
            </code>
            . A UI editor ships once the backend exposes{" "}
            <code className="font-mono text-xs">GET | PUT /firewall</code>.
          </p>
        </div>
      </div>
    </div>
  );
}
