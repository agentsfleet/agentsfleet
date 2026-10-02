// The invite email vocabulary, dependency-free on purpose: client components read it without pulling the transport, whose retry policy is server-only.

/** What became of an invite's most recent email. Mirrors `EMAIL_STATUS_SENT`,
 * `EMAIL_STATUS_FAILED` and `EMAIL_STATUS_UNCONFIGURED` in
 * `rustd/crates/afd_tenant/src/team/invitation/mail_status.rs`. */
export const EMAIL_STATUS = {
  sent: "sent",
  failed: "failed",
  unconfigured: "unconfigured",
} as const;

export type EmailStatus = (typeof EMAIL_STATUS)[keyof typeof EMAIL_STATUS];
