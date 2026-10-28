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
import { PlusIcon } from "lucide-react";
import AddSecretFormDynamic from "@/components/domain/island-dynamic/AddSecretFormDynamic";
import { CREATE_SECRET_TOOLTIP } from "../copy";
import { SecretValueNotice } from "./SecretValueNotice";

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
            Name it and add one or more fields (like api_key). <SecretValueNotice />
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
