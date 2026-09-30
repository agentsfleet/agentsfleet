"use client";

import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Button,
  CopyButton,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
  Time,
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import type { InviteSummary } from "@/lib/api/invites";
import { presentErrorString } from "@/lib/errors";
import { createInviteAction } from "../actions";

// The backend lowercases and checks the address itself; this only spares a
// round trip for something that is plainly not one.
const schema = z.object({
  email: z.string().trim().pipe(z.email("Enter an email address")),
});
type FormValues = z.infer<typeof schema>;

export default function InviteDialog({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [created, setCreated] = useState<InviteSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: { email: "" } });

  // Closing from any path starts the next invite from an empty form.
  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (next) return;
    setCreated(null);
    setError(null);
    form.reset({ email: "" });
  }

  function onSubmit(values: FormValues) {
    setError(null);
    startTransition(async () => {
      const result = await createInviteAction(values.email);
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "create the invite" }));
        return;
      }
      setCreated(result.data);
      onCreated();
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button type="button" size="sm">
          <PlusIcon size={14} />
          Invite
        </Button>
      </DialogTrigger>
      <DialogContent>
        {created ? (
          <InviteReady invite={created} onDone={() => handleOpenChange(false)} />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Invite someone</DialogTitle>
              <DialogDescription>They can open every workspace in your account.</DialogDescription>
            </DialogHeader>
            <Form {...form}>
              <form
                onSubmit={(e) => { void form.handleSubmit(onSubmit)(e); }}
                className="space-y-4"
                // The schema's message, not the browser's bubble, explains a bad address.
                noValidate
              >
                <FormField
                  control={form.control}
                  name="email"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>Email</FormLabel>
                      <FormControl>
                        <Input type="email" placeholder="teammate@example.com" autoComplete="off" {...field} />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
                <DialogFooter>
                  <Button type="button" variant="ghost" disabled={pending} onClick={() => handleOpenChange(false)}>
                    Cancel
                  </Button>
                  <Button type="submit" disabled={pending}>
                    {pending ? <Spinner size="sm" srLabel="Creating" /> : null}
                    Create invite
                  </Button>
                </DialogFooter>
              </form>
            </Form>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

// The link is what the invitee opens, so it shows the moment the invite
// exists. The table keeps a copy action on the row, so closing loses nothing.
function InviteReady({ invite, onDone }: { invite: InviteSummary; onDone: () => void }) {
  return (
    <div className="space-y-4" data-testid="invite-ready">
      <DialogHeader>
        <DialogTitle>Invite ready</DialogTitle>
        <DialogDescription>
          Send {invite.email} this link. It expires <Time value={new Date(invite.expires_at)} format="relative" />.
        </DialogDescription>
      </DialogHeader>
      <div className="flex items-center gap-sm">
        <Input readOnly value={invite.link} aria-label="Invite link" onFocus={(e) => e.currentTarget.select()} />
        <CopyButton value={invite.link} label="Copy invite link" />
      </div>
      <DialogFooter>
        <Button type="button" onClick={onDone}>Done</Button>
      </DialogFooter>
    </div>
  );
}
