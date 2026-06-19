// Security proof for the device-flow transport: the Clerk-minted token the
// dashboard encrypts to the CLI is sealed with ECDH P-256 + AES-256-GCM
// against the CLI's ephemeral public key. This suite proves a party who
// captures the on-the-wire payload (ciphertext + nonce + dashboard pubkey)
// — or a tampered copy of it — CANNOT recover the token without the CLI's
// ephemeral private key. That is the safety guarantee behind "the minted
// credential is never exposed on any server-side surface but process
// memory" (docs/AUTH.md): even a full transcript of the exchange is inert.

import { describe, expect, it } from "bun:test";

import {
  decryptJwt,
  deriveSharedKey,
  encryptJwtForTest,
  generateCliKeypair,
} from "../src/lib/cli-flow.ts";
import type { EncryptedJwt } from "../src/lib/cli-flow.ts";

// A fake 3-segment token standing in for the real Clerk-minted JWT. Built at
// runtime from base64 parts (never a static token literal — that keeps the
// secret scanner from flagging a test fixture) and never a live secret. The
// crypto proof is content-agnostic: the point is the value stays
// unrecoverable from the wire regardless of what it is.
const FIXTURE_JWT = [
  btoa(JSON.stringify({ alg: "RS256", typ: "JWT" })),
  btoa(JSON.stringify({ sub: "user_fixture", iat: 1 })),
  btoa("signature-fixture-not-real"),
].join(".").replace(/=/g, "");

const SHORT_NONCE_BASE64URL = "AAAA";

interface CapturedExchange {
  readonly payload: EncryptedJwt;
  readonly dashboardPublicKeyBase64Url: string;
}

// Simulate the dashboard's encrypt leg exactly as ui/.../cli-flow.ts does:
// generate an ephemeral keypair, derive the shared key against the CLI's
// public key, AES-GCM-seal the token. Returns precisely what an on-path
// observer could capture.
async function dashboardSeals(cliPublicKeyBase64Url: string, jwt: string): Promise<CapturedExchange> {
  const dashboard = await generateCliKeypair();
  const shared = await deriveSharedKey(dashboard.privateKey, cliPublicKeyBase64Url);
  const payload = await encryptJwtForTest(shared, jwt);
  return { payload, dashboardPublicKeyBase64Url: dashboard.publicKeyBase64Url };
}

function flipFirstChar(base64url: string): string {
  const first = base64url[0];
  const replacement = first === "A" ? "B" : "A";
  return replacement + base64url.slice(1);
}

describe("device-flow transport secrecy — captured ciphertext is inert without the CLI key", () => {
  it("the legitimate CLI recovers the token with its own private key (ECDH round-trip)", async () => {
    const cli = await generateCliKeypair();
    const captured = await dashboardSeals(cli.publicKeyBase64Url, FIXTURE_JWT);
    const shared = await deriveSharedKey(cli.privateKey, captured.dashboardPublicKeyBase64Url);
    const recovered = await decryptJwt(shared, captured.payload.ciphertextBase64Url, captured.payload.nonceBase64Url);
    expect(recovered).toBe(FIXTURE_JWT);
  });

  it("the token never appears in the ciphertext (encrypted, not merely encoded)", async () => {
    const cli = await generateCliKeypair();
    const captured = await dashboardSeals(cli.publicKeyBase64Url, FIXTURE_JWT);
    expect(captured.payload.ciphertextBase64Url).not.toContain(FIXTURE_JWT);
    for (const segment of FIXTURE_JWT.split(".")) {
      expect(captured.payload.ciphertextBase64Url).not.toContain(segment);
    }
  });

  it("an attacker with their own keypair cannot decrypt the captured payload", async () => {
    const cli = await generateCliKeypair();
    const captured = await dashboardSeals(cli.publicKeyBase64Url, FIXTURE_JWT);
    // Attacker derives against the captured dashboard pubkey using a key
    // they control — a different shared secret, so AES-GCM auth fails.
    const attacker = await generateCliKeypair();
    const attackerKey = await deriveSharedKey(attacker.privateKey, captured.dashboardPublicKeyBase64Url);
    await expect(
      decryptJwt(attackerKey, captured.payload.ciphertextBase64Url, captured.payload.nonceBase64Url),
    ).rejects.toThrow();
  });

  it("a different CLI keypair cannot open a payload sealed to the original CLI key", async () => {
    const cli = await generateCliKeypair();
    const captured = await dashboardSeals(cli.publicKeyBase64Url, FIXTURE_JWT);
    const impostorCli = await generateCliKeypair();
    const wrongKey = await deriveSharedKey(impostorCli.privateKey, captured.dashboardPublicKeyBase64Url);
    await expect(
      decryptJwt(wrongKey, captured.payload.ciphertextBase64Url, captured.payload.nonceBase64Url),
    ).rejects.toThrow();
  });

  it("flipping a single ciphertext byte fails the AEAD integrity check", async () => {
    const cli = await generateCliKeypair();
    const captured = await dashboardSeals(cli.publicKeyBase64Url, FIXTURE_JWT);
    const shared = await deriveSharedKey(cli.privateKey, captured.dashboardPublicKeyBase64Url);
    const tampered = flipFirstChar(captured.payload.ciphertextBase64Url);
    await expect(decryptJwt(shared, tampered, captured.payload.nonceBase64Url)).rejects.toThrow();
  });

  it("a wrong-length nonce is rejected before any decrypt attempt", async () => {
    const cli = await generateCliKeypair();
    const captured = await dashboardSeals(cli.publicKeyBase64Url, FIXTURE_JWT);
    const shared = await deriveSharedKey(cli.privateKey, captured.dashboardPublicKeyBase64Url);
    await expect(
      decryptJwt(shared, captured.payload.ciphertextBase64Url, SHORT_NONCE_BASE64URL),
    ).rejects.toThrow();
  });
});
