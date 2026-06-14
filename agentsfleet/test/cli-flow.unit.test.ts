// Crypto-mirror tests for the device-authorization flow.
//
// The dashboard side lives in ui/packages/app/lib/auth/cli-flow.ts; the
// CLI side mirrors derivation against the dashboard's public key. Both
// sides must produce the same AES-256-GCM key from the same ECDH +
// HKDF inputs or every login flow fails decrypt — these tests pin the
// constants (HKDF info, salt, curve, AES bits) by exercising the full
// round trip, not by string-comparing the constants.

import { describe, test, expect } from "bun:test";
import { webcrypto } from "node:crypto";
import {
  decryptJwt,
  deriveSharedKey,
  encryptJwtForTest,
  fingerprintHex,
  generateCliKeypair,
} from "../src/lib/cli-flow.ts";

interface PeerKeypair {
  readonly privateKey: CryptoKey;
  readonly publicKeyBase64Url: string;
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

async function generatePeerKeypair(): Promise<PeerKeypair> {
  const pair = await webcrypto.subtle.generateKey(
    { name: "ECDH", namedCurve: "P-256" },
    true,
    ["deriveBits"],
  );
  const spki = await webcrypto.subtle.exportKey("spki", pair.publicKey);
  return {
    privateKey: pair.privateKey,
    publicKeyBase64Url: base64UrlEncode(new Uint8Array(spki)),
  };
}

describe("cli-flow crypto mirror", () => {
  test("generateCliKeypair returns a usable P-256 keypair with SPKI base64url public key", async () => {
    const kp = await generateCliKeypair();
    expect(kp.publicKeyBase64Url.length).toBeGreaterThan(0);
    expect(kp.publicKeyBase64Url).not.toContain("=");
    expect(kp.publicKeyBase64Url).not.toContain("+");
    expect(kp.publicKeyBase64Url).not.toContain("/");
    expect(kp.privateKey.type).toBe("private");
  });

  test("dashboard-side encrypt → CLI-side derive + decrypt round-trip recovers the JWT", async () => {
    const cli = await generateCliKeypair();
    const dashboard = await generatePeerKeypair();

    const dashKey = await deriveSharedKey(
      dashboard.privateKey,
      cli.publicKeyBase64Url,
    );
    const jwt = "eyJhbGciOiJIUzI1NiJ9.payload.sig";
    const { ciphertextBase64Url, nonceBase64Url } = await encryptJwtForTest(
      dashKey,
      jwt,
    );

    const cliKey = await deriveSharedKey(
      cli.privateKey,
      dashboard.publicKeyBase64Url,
    );
    const recovered = await decryptJwt(cliKey, ciphertextBase64Url, nonceBase64Url);
    expect(recovered).toBe(jwt);
  });

  test("decryptJwt with the wrong derived key throws", async () => {
    const cli = await generateCliKeypair();
    const dashboard = await generatePeerKeypair();
    const attacker = await generatePeerKeypair();

    const dashKey = await deriveSharedKey(
      dashboard.privateKey,
      cli.publicKeyBase64Url,
    );
    const { ciphertextBase64Url, nonceBase64Url } = await encryptJwtForTest(
      dashKey,
      "secret-token",
    );

    const wrongKey = await deriveSharedKey(
      cli.privateKey,
      attacker.publicKeyBase64Url,
    );
    await expect(
      decryptJwt(wrongKey, ciphertextBase64Url, nonceBase64Url),
    ).rejects.toThrow();
  });

  test("decryptJwt rejects nonce of wrong length", async () => {
    const cli = await generateCliKeypair();
    const dashboard = await generatePeerKeypair();
    const dashKey = await deriveSharedKey(
      dashboard.privateKey,
      cli.publicKeyBase64Url,
    );
    const { ciphertextBase64Url } = await encryptJwtForTest(dashKey, "jwt");
    const cliKey = await deriveSharedKey(
      cli.privateKey,
      dashboard.publicKeyBase64Url,
    );
    const shortNonce = base64UrlEncode(new Uint8Array(8));
    await expect(
      decryptJwt(cliKey, ciphertextBase64Url, shortNonce),
    ).rejects.toThrow(/nonce length/);
  });

  test("fingerprintHex returns 64-char lowercase hex sha256", async () => {
    const hex = await fingerprintHex("session-id-abc");
    expect(hex).toMatch(/^[0-9a-f]{64}$/);
  });

  test("fingerprintHex is deterministic for the same input", async () => {
    const a = await fingerprintHex("input");
    const b = await fingerprintHex("input");
    expect(a).toBe(b);
  });
});
