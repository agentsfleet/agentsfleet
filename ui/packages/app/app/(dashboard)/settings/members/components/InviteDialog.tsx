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
import { EMAIL_STATUS, type EmailStatus, type InviteSummary } from "@/lib/api/invites";
import { presentErrorString } from "@/lib/errors";
import { createInviteAction } from "../actions";

// The backend lowercases and checks the address itself; this only spares a
// round trip for something that is plainly not one.
const ENTER_AN_ADDRESS = "Enter an email address";
// RFC 5321 caps the part before the @ at 64 characters, and the backend's mail
// parser refuses a longer one; zod's email rule alone lets it through.
const LOCAL_PART_MAX = 64;
const schema = z.object({
  email: z
    .string()
    .trim()
    .pipe(z.email(ENTER_AN_ADDRESS))
    .refine((email) => email.lastIndexOf("@") <= LOCAL_PART_MAX, ENTER_AN_ADDRESS),
});
type FormValues = z.infer<typeof schema>;

/** `onSettled` runs after every create attempt: a refused or timed-out create
 * may still have saved the invite, so the list re-reads either way. */
export default function InviteDialog({ onSettled }: { onSettled: () => void }) {
  const [open, setOpen] = useState(false);
  const [created, setCreated] = useState<InviteSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: { email: "" } });

  // Closing from any path starts the next invite from an empty form. A send in
  // flight finishes here first, or its answer would land in the next invite.
  function handleOpenChange(next: boolean) {
    if (!next && pending) return;
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
      onSettled();
      if (!result.ok) {
        setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "create the invite" }));
        return;
      }
      setCreated(result.data);
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
                    {pending ? <Spinner size="sm" srLabel="Sending" /> : null}
                    Send
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

const INVITE_CREATED = "Invite created";

// Only a relay that accepted the email earns "sent". An invite whose email
// failed, or that has no relay to send it, still exists, and its link is the
// way in.
const READY_COPY: Record<EmailStatus, { title: string; lead: (email: string) => string }> = {
  [EMAIL_STATUS.sent]: {
    title: "Invitation sent",
    lead: (email) => `We emailed ${email}. You can also copy the link and share it.`,
  },
  [EMAIL_STATUS.failed]: {
    title: INVITE_CREATED,
    lead: (email) => `The email to ${email} did not go out. Copy the link and share it, or send the email again from the list.`,
  },
  [EMAIL_STATUS.unconfigured]: {
    title: INVITE_CREATED,
    lead: (email) => `This deployment sends no email. Copy the link and share it with ${email}.`,
  },
};

// The link shows the moment the invite exists. The table keeps a copy action
// on the row, so closing loses nothing.
function InviteReady({ invite, onDone }: { invite: InviteSummary; onDone: () => void }) {
  const copy = READY_COPY[invite.email_status];
  return (
    <div className="space-y-4" data-testid="invite-ready">
      <DialogHeader>
        <DialogTitle>{copy.title}</DialogTitle>
        <DialogDescription>
          {copy.lead(invite.email)} It expires <Time value={new Date(invite.expires_at)} format="relative" />.
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
