"use client";

import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  TooltipButton,
} from "@agentsfleet/design-system";
import { CircleHelpIcon, PlusIcon } from "lucide-react";
import AddSecretFormDynamic from "@/components/domain/island-dynamic/AddSecretFormDynamic";
import { CREATE_SECRET_TOOLTIP } from "../copy";

const ADD_SECRET_TRIGGER_LABEL = "Create secret";

export default function AddSecretDialog({ workspaceId }: { workspaceId: string }) {
  const [open, setOpen] = useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <TooltipButton type="button" size="sm" tooltip={CREATE_SECRET_TOOLTIP}>
          <PlusIcon size={14} />
          {ADD_SECRET_TRIGGER_LABEL}
        </TooltipButton>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create secret</DialogTitle>
          <DialogDescription>
            Name it and add one or more fields (like api_key). Values are encrypted on save — you
            can replace them later, but never view them again.{" "}
            <a
              href="https://docs.agentsfleet.net/fleets/credentials"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-pulse underline-offset-2 hover:underline focus-visible:underline"
            >
              <CircleHelpIcon size={13} aria-hidden="true" />
              Learn more<span className="sr-only"> (opens in a new tab)</span>
            </a>
          </DialogDescription>
        </DialogHeader>
        <AddSecretFormDynamic
          workspaceId={workspaceId}
          onDone={() => setOpen(false)}
          onCancel={() => setOpen(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
