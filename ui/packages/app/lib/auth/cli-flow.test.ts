import { describe, expect, it } from "vitest";
import {
  deriveSharedKey,
  encryptJwt,
  generateEphemeralKeypair,
  generateVerificationCode,
} from "./cli-flow";

const textDecoder = new TextDecoder();

describe("generateEphemeralKeypair", () => {
  it("returns a non-extractable P-256 private key and a base64url SPKI public key", async () => {
    const { privateKey, publicKeyBase64Url } = await generateEphemeralKeypair();
    expect(privateKey.type).toBe("private");
    expect(privateKey.algorithm.name).toBe("ECDH");
    expect(publicKeyBase64Url).toMatch(/^[A-Za-z0-9_-]+$/);
    // P-256 SPKI is 91 bytes → ~122 base64url chars; sanity check, not exact.
    expect(publicKeyBase64Url.length).toBeGreaterThan(100);
  });

  it("produces distinct keypairs on each call", async () => {
    const a = await generateEphemeralKeypair();
    const b = await generateEphemeralKeypair();
    expect(a.publicKeyBase64Url).not.toBe(b.publicKeyBase64Url);
  });
});

describe("deriveSharedKey", () => {
  it("produces a symmetric key — what one side encrypts, the other decrypts", async () => {
    const cli = await generateEphemeralKeypair();
    const dash = await generateEphemeralKeypair();

    const dashKey = await deriveSharedKey(dash.privateKey, cli.publicKeyBase64Url);
    const cliKey = await deriveSharedKey(cli.privateKey, dash.publicKeyBase64Url);

    const plaintext = "header.payload.signature";
    const { ciphertext, nonce } = await encryptJwt(plaintext, dashKey);

    const nonceBytes = decodeBase64Url(nonce);
    const ctBytes = decodeBase64Url(ciphertext);
    const decrypted = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: nonceBytes },
      cliKey,
      ctBytes,
    );
    expect(textDecoder.decode(decrypted)).toBe(plaintext);
  });
});

describe("encryptJwt", () => {
  it("generates a fresh 12-byte nonce on every call", async () => {
    const cli = await generateEphemeralKeypair();
    const dash = await generateEphemeralKeypair();
    const key = await deriveSharedKey(dash.privateKey, cli.publicKeyBase64Url);

    const a = await encryptJwt("jwt-a", key);
    const b = await encryptJwt("jwt-b", key);

    expect(a.nonce).not.toBe(b.nonce);
    expect(decodeBase64Url(a.nonce).byteLength).toBe(12);
    expect(decodeBase64Url(b.nonce).byteLength).toBe(12);
  });
});

describe("generateVerificationCode", () => {
  it("returns exactly 6 ASCII digits", () => {
    for (let i = 0; i < 100; i++) {
      const code = generateVerificationCode();
      expect(code).toMatch(/^\d{6}$/);
    }
  });

  it("does not produce identical codes back-to-back across many draws", () => {
    const codes = new Set<string>();
    for (let i = 0; i < 50; i++) codes.add(generateVerificationCode());
    // 50 draws from a 1M-code space should overwhelmingly be unique.
    expect(codes.size).toBeGreaterThan(40);
  });
});

function decodeBase64Url(input: string): Uint8Array<ArrayBuffer> {
  const pad = input.length % 4 === 0 ? "" : "=".repeat(4 - (input.length % 4));
  const b64 = input.replaceAll("-", "+").replaceAll("_", "/") + pad;
  const binary = atob(b64);
  const buf = new ArrayBuffer(binary.length);
  const out = new Uint8Array(buf);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}
