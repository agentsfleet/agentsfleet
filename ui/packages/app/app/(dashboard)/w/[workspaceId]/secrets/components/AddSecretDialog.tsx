"use client";

import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Button,
} from "@agentsfleet/design-system";
import { PlusIcon } from "@phosphor-icons/react/ssr";
import AddSecretFormDynamic from "@/components/domain/island-dynamic/AddSecretFormDynamic";
import { SecretValueNotice } from "./SecretValueNotice";

const ADD_SECRET_TRIGGER_LABEL = "Add secret";

export default function AddSecretDialog({ workspaceId }: { workspaceId: string }) {
  const [open, setOpen] = useState(false);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button type="button" size="sm">
          <PlusIcon size={16} />
          {ADD_SECRET_TRIGGER_LABEL}
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add secret</DialogTitle>
          <DialogDescription>
            <SecretValueNotice />
          </DialogDescription>
        </DialogHeader>
        <AddSecretFormDynamic
          workspaceId={workspaceId}
          showValueNotice={false}
          onDone={() => setOpen(false)}
          onCancel={() => setOpen(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
