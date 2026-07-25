export type WorkspaceCreateAttempt = {
  idempotencyKey: string;
  name: string | undefined;
};

const IDEMPOTENCY_KEY_STORAGE = "agentsfleet.workspace-create.idempotency-key";
const REQUEST_NAME_STORAGE = "agentsfleet.workspace-create.request-name";
const UUID_V7_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const UUID_TIMESTAMP_HEX_LENGTH = 12;

export function acquireWorkspaceCreateAttempt(
  name: string | undefined,
  fallback: WorkspaceCreateAttempt | null,
): WorkspaceCreateAttempt {
  const normalizedName = normalizeName(name);
  if (fallback && fallback.name === normalizedName) return fallback;

  const stored = readStoredAttempt();
  if (stored && stored.name === normalizedName) return stored;

  const attempt = {
    idempotencyKey: createUuidV7(),
    name: normalizedName,
  };
  storeAttempt(attempt);
  return attempt;
}

export function clearWorkspaceCreateAttempt(
  attempt: WorkspaceCreateAttempt,
): void {
  const stored = readStoredAttempt();
  if (stored?.idempotencyKey !== attempt.idempotencyKey) return;
  try {
    window.sessionStorage.removeItem(IDEMPOTENCY_KEY_STORAGE);
    window.sessionStorage.removeItem(REQUEST_NAME_STORAGE);
  } catch {
    // Browser storage is best-effort; the controller keeps an in-memory copy.
  }
}

function normalizeName(name: string | undefined): string | undefined {
  const trimmed = name?.trim();
  return trimmed ? trimmed : undefined;
}

function readStoredAttempt(): WorkspaceCreateAttempt | null {
  try {
    const idempotencyKey = window.sessionStorage.getItem(
      IDEMPOTENCY_KEY_STORAGE,
    );
    if (!idempotencyKey || !UUID_V7_PATTERN.test(idempotencyKey)) return null;
    const storedName = window.sessionStorage.getItem(REQUEST_NAME_STORAGE);
    return {
      idempotencyKey,
      name: storedName ? storedName : undefined,
    };
  } catch {
    return null;
  }
}

function storeAttempt(attempt: WorkspaceCreateAttempt): void {
  try {
    window.sessionStorage.setItem(
      IDEMPOTENCY_KEY_STORAGE,
      attempt.idempotencyKey,
    );
    window.sessionStorage.setItem(REQUEST_NAME_STORAGE, attempt.name ?? "");
  } catch {
    // The in-memory fallback still protects retries in this mounted session.
  }
}

function createUuidV7(): string {
  const timestamp = Date.now()
    .toString(16)
    .padStart(UUID_TIMESTAMP_HEX_LENGTH, "0")
    .slice(-UUID_TIMESTAMP_HEX_LENGTH);
  const random = crypto.randomUUID();
  return `${timestamp.slice(0, 8)}-${timestamp.slice(8)}-7${random.slice(15)}`;
}
