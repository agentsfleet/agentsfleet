"use client";

import { useState } from "react";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@agentsfleet/design-system";
import AddSecretFormDynamic from "@/components/domain/island-dynamic/AddSecretFormDynamic";

const ADD_SECRET_TRIGGER_LABEL = "Add Secret";

export default function AddSecretDialog({ workspaceId }: { workspaceId: string }) {
  const [open, setOpen] = useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button type="button" size="sm">
          {ADD_SECRET_TRIGGER_LABEL}
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add a secret</DialogTitle>
          <DialogDescription>
            Name it and add one or more fields (like api_key). Values are encrypted on save — you
            can replace them later, but never view them again.{" "}
            <a
              href="https://docs.agentsfleet.net/fleets/credentials"
              target="_blank"
              rel="noopener noreferrer"
              className="text-pulse underline-offset-2 hover:underline focus-visible:underline"
            >
              Learn more<span className="sr-only"> (opens in a new tab)</span>
            </a>
          </DialogDescription>
        </DialogHeader>
        <AddSecretFormDynamic workspaceId={workspaceId} onDone={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}
