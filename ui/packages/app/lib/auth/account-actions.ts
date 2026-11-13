"use server";

import { readAccountSessions, revokeAccountSession } from "./credential";

export async function listAccountSessionsAction(expectedSubject: string, offset = 0) {
  return readAccountSessions(expectedSubject, offset);
}

export async function revokeAccountSessionAction(expectedSubject: string, id: string) {
  return revokeAccountSession(expectedSubject, id);
}
