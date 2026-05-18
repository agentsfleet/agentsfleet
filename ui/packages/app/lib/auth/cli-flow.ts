// Browser-side crypto for the CLI device-authorization flow.
//
// The dashboard imports the CLI's P-256 public key from the session row,
// generates its own ephemeral P-256 keypair, derives a shared secret via
// ECDH, runs it through HKDF-SHA256, and uses the resulting AES-256-GCM key
// to encrypt the Clerk-minted JWT. The ciphertext + nonce + dashboard
// public key + a 6-digit verification code are PATCHed to the server; the
// CLI then mirrors the same derivation on its end to decrypt.
//
// Pure `crypto.subtle` — no extra deps. The HKDF info string is pinned by
// RULE UFS: a single named constant, never inlined. The CLI side reads the
// same constant from `zombiectl`.

const HKDF_INFO_STRING = "m74-002-v1" as const;
const ECDH_CURVE = "P-256" as const;
const AES_GCM_BITS = 256 as const;
const NONCE_BYTES = 12 as const;
const VERIFICATION_CODE_DIGITS = 6 as const;
const VERIFICATION_CODE_MODULUS = 1_000_000 as const;

const textEncoder = new TextEncoder();

export interface EphemeralKeypair {
  privateKey: CryptoKey;
  publicKeyBase64Url: string;
}

export interface EncryptedJwt {
  ciphertext: string;
  nonce: string;
}

export async function generateEphemeralKeypair(): Promise<EphemeralKeypair> {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDH", namedCurve: ECDH_CURVE },
    true,
    ["deriveBits"],
  );
  const spki = await crypto.subtle.exportKey("spki", pair.publicKey);
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
  const peerPublicKey = await crypto.subtle.importKey(
    "spki",
    peerSpki,
    { name: "ECDH", namedCurve: ECDH_CURVE },
    false,
    [],
  );
  const sharedBits = await crypto.subtle.deriveBits(
    { name: "ECDH", public: peerPublicKey },
    privateKey,
    AES_GCM_BITS,
  );
  const hkdfBase = await crypto.subtle.importKey(
    "raw",
    sharedBits,
    "HKDF",
    false,
    ["deriveKey"],
  );
  return crypto.subtle.deriveKey(
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

export async function encryptJwt(jwt: string, key: CryptoKey): Promise<EncryptedJwt> {
  const nonce = crypto.getRandomValues(new Uint8Array(NONCE_BYTES));
  const ciphertextBuf = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: nonce },
    key,
    textEncoder.encode(jwt),
  );
  return {
    ciphertext: base64UrlEncode(new Uint8Array(ciphertextBuf)),
    nonce: base64UrlEncode(nonce),
  };
}

export function generateVerificationCode(): string {
  const arr = new Uint32Array(1);
  crypto.getRandomValues(arr);
  let raw = 0;
  for (const v of arr) raw = v;
  return (raw % VERIFICATION_CODE_MODULUS).toString().padStart(VERIFICATION_CODE_DIGITS, "0");
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
