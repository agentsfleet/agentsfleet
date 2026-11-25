import { useRef, useState } from "react";
import { BrandMark, Button, Card } from "@agentsfleet/design-system";
import { GithubLogoIcon } from "@phosphor-icons/react/dist/ssr/GithubLogo";
import { StackIcon } from "@phosphor-icons/react/dist/ssr/Stack";
import { FleetSummary } from "./FleetSummary";

type Access = "pending" | "approved" | "denied";

const ACCESS_STATUS: Record<Access, string> = {
  pending: "GitHub access requested",
  approved: "GitHub access approved",
  denied: "GitHub access denied",
};

export function ProductDemo() {
  const [access, setAccess] = useState<Access>("pending");

  return (
    <Card asChild className="rounded-lg p-0">
      <figure aria-label="Example fleet conversation">
        <figcaption className="flex flex-wrap items-center justify-between gap-md border-b border-border px-xl py-lg sm:px-2xl">
          <span className="inline-flex items-center gap-md font-semibold">
            <StackIcon aria-hidden="true" className="size-4 text-text-muted" />
            Release watch
          </span>
          <span className="text-eyebrow leading-eyebrow text-text-subtle">Interactive example</span>
        </figcaption>
        <div className="grid gap-xl bg-background p-xl sm:p-2xl">
          <div className="grid gap-xl">
            <div className="justify-self-end rounded-md bg-muted px-xl py-lg text-conversation leading-conversation">
              <span className="sr-only">You: </span>
              Tell me when a pull request merges in acme/web.
            </div>
            <div className="grid gap-md">
              <p className="flex items-center gap-md text-label font-semibold">
                <BrandMark className="size-4 text-pulse" />
                Release watch
              </p>
              <p className="text-conversation leading-conversation text-text-chat">
                I can watch for merges and report here. I’ll need access to the repository.
              </p>
            </div>
          </div>
          <AccessCard access={access} onAccess={setAccess} />
        </div>
        <FleetSummary approved={access === "approved"} />
      </figure>
    </Card>
  );
}

function AccessCard({ access, onAccess }: { access: Access; onAccess: (access: Access) => void }) {
  const primary = useRef<HTMLButtonElement>(null);
  const pending = access === "pending";
  return (
    <Card asChild className="grid gap-md p-xl">
      <div>
        <output className="flex items-center gap-md text-label font-semibold">
          <GithubLogoIcon aria-hidden="true" className="size-4 shrink-0 text-text-muted" />
          {ACCESS_STATUS[access]}
        </output>
        <p className="text-label text-text-muted">
          <span className="font-mono">acme/web</span> · Read access
        </p>
        <div className="flex flex-wrap gap-md pt-sm">
          <Button
            ref={primary}
            variant={pending ? "default" : "outline"}
            size="compact"
            onClick={() => onAccess(pending ? "approved" : "pending")}
          >
            {pending ? "Approve" : "Reset example"}
          </Button>
          {pending && (
            <Button
              variant="outline"
              size="compact"
              onClick={() => {
                primary.current?.focus({ preventScroll: true });
                onAccess("denied");
              }}
            >
              Deny
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}
