// CLI-side crypto mirror for the device-authorization flow.
//
// Pairs with ui/packages/app/lib/auth/cli-flow.ts. The dashboard generates
// its ephemeral P-256 keypair, derives a shared secret against the CLI's
// public key, HKDF-expands with an empty salt + the pinned info string,
// AES-256-GCM-encrypts the Clerk-minted JSON Web Token (JWT). The CLI
// mirrors the derivation here with its own private key against the
// dashboard public key and AES-GCM-decrypts the ciphertext to recover the
// JWT.
//
// Constants below are pinned by name (RULE UFS): any drift between the
// dashboard's HKDF_INFO_STRING / ECDH_CURVE / AES_GCM_BITS / NONCE_BYTES
// and the CLI's would produce different AES keys on the two sides and
// every login flow would fail decrypt. The empty-Uint8Array HKDF salt
// matches the dashboard's exact byte sequence; the RFC 5869 no-salt
// default of HashLen zero bytes is *not* what Web Crypto does when given
// `new Uint8Array(0)`.

import { webcrypto } from "node:crypto";

const HKDF_INFO_STRING = "m74-002-v1" as const;
const ECDH_CURVE = "P-256" as const;
const AES_GCM_BITS = 256 as const;
const NONCE_BYTES = 12 as const;

const subtle: SubtleCrypto = webcrypto.subtle;
const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder("utf-8", { fatal: true });

export interface CliKeypair {
  readonly privateKey: CryptoKey;
  readonly publicKeyBase64Url: string;
}

export interface EncryptedJwt {
  readonly ciphertextBase64Url: string;
  readonly nonceBase64Url: string;
}

export async function generateCliKeypair(): Promise<CliKeypair> {
  const pair = await subtle.generateKey(
    { name: "ECDH", namedCurve: ECDH_CURVE },
    true,
    ["deriveBits"],
  );
  const spki = await subtle.exportKey("spki", pair.publicKey);
  return {
    privateKey: pair.privateKey,
    publicKeyBase64Url: base64UrlEncode(new Uint8Array(spki)),
  };
}

export async function deriveSharedKey(
  privateKey: CryptoKey,
  peerPublicKeyBase64Url: string,
): Promise<CryptoKey> {
  const peerSpki = base64UrlDecode(peerPublicKeyBase64Url);
  const peerPublicKey = await subtle.importKey(
    "spki",
    peerSpki,
    { name: "ECDH", namedCurve: ECDH_CURVE },
    false,
    [],
  );
  const sharedBits = await subtle.deriveBits(
    { name: "ECDH", public: peerPublicKey },
    privateKey,
    AES_GCM_BITS,
  );
  const hkdfBase = await subtle.importKey(
    "raw",
    sharedBits,
    "HKDF",
    false,
    ["deriveKey"],
  );
  return subtle.deriveKey(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: new Uint8Array(0),
      info: textEncoder.encode(HKDF_INFO_STRING),
    },
    hkdfBase,
    { name: "AES-GCM", length: AES_GCM_BITS },
    false,
    ["encrypt", "decrypt"],
  );
}

export async function decryptJwt(
  key: CryptoKey,
  ciphertextBase64Url: string,
  nonceBase64Url: string,
): Promise<string> {
  const ciphertext = base64UrlDecode(ciphertextBase64Url);
  const nonce = base64UrlDecode(nonceBase64Url);
  if (nonce.byteLength !== NONCE_BYTES) {
    throw new Error(`nonce length ${nonce.byteLength} != ${NONCE_BYTES}`);
  }
  const plaintext = await subtle.decrypt(
    { name: "AES-GCM", iv: nonce },
    key,
    ciphertext,
  );
  return textDecoder.decode(plaintext);
}

// Client fingerprint = sha256(remote_addr || user_agent || session_id),
// lowercase hex. On the CLI side we don't have a server-visible remote
// address; the server computes the canonical fingerprint from its own
// view of the request. The CLI never needs to compute or send the
// fingerprint — this helper is exported for testing parity only.
export async function fingerprintHex(input: string): Promise<string> {
  const bytes = await subtle.digest("SHA-256", textEncoder.encode(input));
  return Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("");
}

export async function encryptJwtForTest(
  key: CryptoKey,
  jwt: string,
): Promise<EncryptedJwt> {
  const nonce = webcrypto.getRandomValues(new Uint8Array(NONCE_BYTES));
  const ciphertext = await subtle.encrypt(
    { name: "AES-GCM", iv: nonce },
    key,
    textEncoder.encode(jwt),
  );
  return {
    ciphertextBase64Url: base64UrlEncode(new Uint8Array(ciphertext)),
    nonceBase64Url: base64UrlEncode(nonce),
  };
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

function base64UrlDecode(input: string): Uint8Array<ArrayBuffer> {
  const pad = "=".repeat((4 - (input.length % 4)) % 4);
  const b64 = input.replaceAll("-", "+").replaceAll("_", "/") + pad;
  const binary = atob(b64);
  const buf = new ArrayBuffer(binary.length);
  const out = new Uint8Array(buf);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}
