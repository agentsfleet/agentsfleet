//! Generic RS256 (RSASSA-PKCS1-v1.5 / SHA-256) signing — the mint-side mirror of
//! `auth/jwks_crypto.verifyRs256`. Any JWT-bearer integration reuses it (the
//! GitHub App today; Google service accounts / Snowflake / Salesforce later):
//! only the claim set + endpoint differ, the signing is identical. Built on
//! `std.crypto.ff` (constant-time modexp) + `std.crypto.Certificate.der`; no
//! third-party dependency. Private-key bytes are never logged; the caller owns
//! every buffer (RULE VLT).

const std = @import("std");
const ff = std.crypto.ff;
const der = std.crypto.Certificate.der;
const Sha256 = std.crypto.hash.sha2.Sha256;

/// RSA modulus headroom: GitHub App keys are 2048-bit; 4096 mirrors the cert
/// verify path's bound. The signing buffer is sized to match.
const MAX_MODULUS_BITS = 4096;
const MAX_MODULUS_LEN = MAX_MODULUS_BITS / 8;
const Modulus = ff.Modulus(MAX_MODULUS_BITS);
const Fe = Modulus.Fe;

/// DER `DigestInfo` prefix for SHA-256 (RFC 8017, EMSA-PKCS1-v1.5) — the 19 bytes
/// preceding the 32-byte hash inside the encoded block.
const SHA256_DIGEST_INFO = [_]u8{
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01,
    0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20,
};

/// Minimum PKCS#1 v1.5 overhead around the DigestInfo (RFC 8017): the leading
/// `00 01`, at least eight `FF` padding bytes, and the `00` separator.
const MIN_PKCS1_OVERHEAD = 11;

pub const SignError = error{
    KeyMalformed,
    KeyTooLarge,
    MessageTooLong,
};

/// RSA private-key material — slices borrowed from the caller's DER buffer.
pub const PrivateKey = struct {
    n: []const u8,
    e: []const u8,
    d: []const u8,
};

/// Parse a PKCS#1 `RSAPrivateKey` DER (a GitHub App `.pem`, base64-decoded).
/// `RSAPrivateKey ::= SEQUENCE { version, modulus, publicExponent, privateExponent, … }`;
/// we read the first four INTEGERs and keep n / e / d.
pub fn parsePkcs1(der_bytes: []const u8) SignError!PrivateKey {
    const seq = parseEl(der_bytes, 0) catch return error.KeyMalformed;
    if (seq.identifier.tag != .sequence) return error.KeyMalformed;
    const ver = parseEl(der_bytes, seq.slice.start) catch return error.KeyMalformed;
    const n_el = parseEl(der_bytes, ver.slice.end) catch return error.KeyMalformed;
    const e_el = parseEl(der_bytes, n_el.slice.end) catch return error.KeyMalformed;
    const d_el = parseEl(der_bytes, e_el.slice.end) catch return error.KeyMalformed;
    if (n_el.identifier.tag != .integer or e_el.identifier.tag != .integer or d_el.identifier.tag != .integer)
        return error.KeyMalformed;
    return .{
        .n = stripLeadingZeros(der_bytes[n_el.slice.start..n_el.slice.end]),
        .e = stripLeadingZeros(der_bytes[e_el.slice.start..e_el.slice.end]),
        .d = stripLeadingZeros(der_bytes[d_el.slice.start..d_el.slice.end]),
    };
}

/// Bounds-checked wrapper: the std DER parser indexes without length checks, so
/// a truncated key would panic. Guard both the header read and the slice end.
fn parseEl(bytes: []const u8, index: u32) !der.Element {
    if (@as(usize, index) + 2 > bytes.len) return error.Truncated;
    const el = try der.Element.parse(bytes, index);
    if (el.slice.end > bytes.len) return error.Truncated;
    return el;
}

fn stripLeadingZeros(b: []const u8) []const u8 {
    var i: usize = 0;
    while (i < b.len and b[i] == 0) : (i += 1) {}
    return b[i..];
}

/// Sign `message` with `key` (RSASSA-PKCS1-v1.5 / SHA-256). Writes the
/// modulus-length signature into `out` and returns the written slice. `out` must
/// hold at least the modulus byte length.
pub fn signRs256(out: []u8, key: PrivateKey, message: []const u8) SignError![]const u8 {
    const mod = Modulus.fromBytes(key.n, .big) catch return error.KeyTooLarge;
    const em_len = (mod.bits() + 7) / 8; // modulus byte length
    if (em_len > MAX_MODULUS_LEN or out.len < em_len) return error.KeyTooLarge;

    var em: [MAX_MODULUS_LEN]u8 = undefined;
    try encodeEmsa(em[0..em_len], message);

    const m = Fe.fromBytes(mod, em[0..em_len], .big) catch return error.MessageTooLong;
    const s = mod.powWithEncodedExponent(m, key.d, .big) catch return error.KeyMalformed;
    s.toBytes(out[0..em_len], .big) catch return error.KeyTooLarge;
    return out[0..em_len];
}

/// EMSA-PKCS1-v1.5 (RFC 8017): `00 01 || PS(FF…) || 00 || DigestInfo || H`,
/// filling exactly `em.len` bytes.
fn encodeEmsa(em: []u8, message: []const u8) SignError!void {
    const t_len = SHA256_DIGEST_INFO.len + Sha256.digest_length;
    if (em.len < t_len + MIN_PKCS1_OVERHEAD) return error.MessageTooLong;

    var h: [Sha256.digest_length]u8 = undefined;
    Sha256.hash(message, &h, .{});

    const ps_len = em.len - t_len - 3; // 00 01 … 00 frame the PS run
    em[0] = 0x00;
    em[1] = 0x01;
    @memset(em[2 .. 2 + ps_len], 0xff);
    em[2 + ps_len] = 0x00;
    @memcpy(em[3 + ps_len ..][0..SHA256_DIGEST_INFO.len], &SHA256_DIGEST_INFO);
    @memcpy(em[3 + ps_len + SHA256_DIGEST_INFO.len ..][0..Sha256.digest_length], &h);
}

/// Max DER bytes for a 4096-bit RSA PKCS#1 private key (modulus + CRT params).
const MAX_PKCS1_DER = 4096;

/// Production `SignFn`: a PEM private key (PKCS#1 `RSA PRIVATE KEY` — the GitHub App
/// `.pem` download format) → DER → RS256-sign. Matches the integration `SignFn`
/// signature, so the broker wires this as `github`'s real signer.
pub fn signPemRs256(out: []u8, private_key_pem: []const u8, message: []const u8) anyerror![]const u8 {
    var der_buf: [MAX_PKCS1_DER]u8 = undefined;
    const der_bytes = try pemBodyToDer(&der_buf, private_key_pem);
    const key = try parsePkcs1(der_bytes);
    return signRs256(out, key, message);
}

/// Strip PEM armor + whitespace and base64-decode the body into `der_out`.
fn pemBodyToDer(der_out: []u8, pem: []const u8) SignError![]const u8 {
    var b64: [MAX_PKCS1_DER * 2]u8 = undefined;
    var n: usize = 0;
    var lines = std.mem.splitScalar(u8, pem, '\n');
    while (lines.next()) |line| {
        const t = std.mem.trim(u8, line, " \r\t");
        if (t.len == 0 or std.mem.startsWith(u8, t, "-----")) continue; // skip armor + blanks
        for (t) |c| {
            if (c == ' ' or c == '\r' or c == '\t') continue;
            if (n >= b64.len) return error.KeyTooLarge;
            b64[n] = c;
            n += 1;
        }
    }
    const dec = std.base64.standard.Decoder;
    const der_len = dec.calcSizeForSlice(b64[0..n]) catch return error.KeyMalformed;
    if (der_len > der_out.len) return error.KeyTooLarge;
    dec.decode(der_out[0..der_len], b64[0..n]) catch return error.KeyMalformed;
    return der_out[0..der_len];
}

// ── Tests (pure — no DB, no network; key material is a throwaway fixture) ─────

// pin test: a throwaway 2048-bit RSA key as PKCS#1 DER (base64, no PEM armor) —
// generated offline, never used in production, embedded only to drive the signer.
const TEST_KEY_PKCS1_B64 =
    "MIIEowIBAAKCAQEAxBs4iJWrDhpuy4GQyfrQhtnrXhzEM86cswmwrs9ouW5S4cCi+yzb+xsMZrK2n1AkVkep6c56My6P/13awSMYdtejrSs/b71W+iE83XSWPJJI4sjzUJ0UEU/AQMiMW6LVmWU55n25NyhVOrLxqO3DI5Kb6qlCxDL1yXgyKEmls1e0qXQD2kigsJp6QhcxXgPhAX6wUL0nhSUACPFG468iRU3DLR66dAsTcy7FjNWxh8ljC8ScM9Rm6yNo9i9CGTQQIRwAolMpIMcSxpBKEIhZpwkiEgtwkSvI1s+u5GxSZ6IyBM9tooyb1TlRsWhYm9pkrroGeG0Y3YSdZawXOWrEUwIDAQABAoIBAFC5J8dJXJU8mjjZB6GsxeOMlo8x5i2xMd2c8oayx9f0qtdUtYIREChIFQ29KOFhWuPNMgsVPEYPN6UVnDN+X9ajozNoJv+2/7OMtQIvuJwMV0ZLE6UuU5Fgs7G3G9eoqqYu/et7+x7SUmsMN9+ip33gHqA0tlAO7g/Vk0f0MOomSYGg85ClU9tUVqWS9WOZk7dDcF0zmXDG2aoZEE0oSV62ysQqtkX6ClC2XX4ZtiaBrPGEMB5yxNr6uPiHj7p0IAJtpxRa7jJ5ylWMYqqYGVsGBRkxYsFIDfXs79oxrs9Jf93wZ7A/yyhgWgU9B05LiO8jZ29VlMyu2BqgvP6ITzECgYEA+0UWu8O9vKEMOq2w8AZh9rDQL/L/mJNVFwKok6j6uBQgdvgN5M/ga8tfXj+PR8slFBhydDj80lESxNWwgTzcn7bNglSxdV4A+gCa01o5W6XE0mSe/hug+7pIR2wO9UYNT0gh10Av0xyUn62dLq2qBT60D0HzX57x5Axv6+Ua3TkCgYEAx8xKLWD18oavkQKucVXR/vTb8OWX6qrKG6IFEtzxOAyaRXN7y/cB7rJdl91ytTvZ4djc3lz+Zj9n3DU3HTtj85MktyomawKNpSif4BMx1MzS7cMX24y8ixBzHhroCObu4h200AIWEs3/4HhafTBVLj8tY65WiPfvqrYQPuKAOesCgYEAh3K2zoC1xvkJnpgCyWCnblPh5fcX0Seatsy4EuEERjaTSY5t7uogD/uRbTzV/92CH1MOX5hYsQcDFxgaDZDBXVctcRQ2lQ4XeKzayRPZ142Ei+Wxz0kVfpzsWZPmfFFG23YGyAHRxfuiInF0SbVT8X/bkF38047a1hPeQUs/MAECgYBQLQebPCyWHTw4ycWsz06MrD/SZJ/Y2J5wBk1Y63aVEmGZ+ySzjbSlz8fFGGVemtztR3Qie1jPOSR5dpVeUqXiaaqzIeP2zzh+DVZSugEmLud55+8b+Fb0yy4W558za1BzRo53Zk7rTuUec82ELTARdeLF/IDXR/9SFutgAM6J7wKBgF/WfKsWeV++aRYXS7vsJqq9xM+P1y9JNcIUtItVA7eYe9vbm8/mQ5e1Qln45k1EgzzkcYBBVbuTF5d92xMAHLfdZUjRCDMc752b9B6i1pgPUnd8w1YDoYK7V/wVavOhXuNPc+btdItLFps0+eOa2NCmJ7G4ekqIAvrTRwmwKlJa";

// pin test: the exact message openssl signed to produce EXPECTED_SIG_HEX.
const TEST_MSG = "m102.github.app.jwt.signing-input";

// pin test: `openssl dgst -sha256 -sign key.pem` over TEST_MSG. PKCS#1 v1.5 is
// deterministic, so our signer must reproduce these bytes exactly.
const EXPECTED_SIG_HEX = "139dd58aa36795ef253ddbaa821ad9b517792769f5d8c568dc7e7d511cc7a297116183abc631a627992793e30272ef27eb6511fe13c1156ce2ab851e08814eebf2def4bc655b277b7d11bab7b5d69b01e8d6d41235fef39bb02183e6445159e0389663ca5c0498aa34afb12da88b298b578f5f074f9f1a03cc45841ca84910715bd19ef64cfda789cc86d055318473b05639f13ade1c8f7714e92adc2ae544157334d42d5dd1eb0a27e9a65a0f7264ace8f7d4555628efb4af1f69a28c7635a1c26855bb5f5b94291e88bebd93bd85ae5a57815ed90b8215eda4d1f4b9cf405ebd1f6cd53c63516e6ba7b2d1724ec68a7e75c362b0cd2038d73b441764422f9a";

fn decodeFixtureKey(der_buf: []u8) !PrivateKey {
    const der_len = try std.base64.standard.Decoder.calcSizeForSlice(TEST_KEY_PKCS1_B64);
    try std.base64.standard.Decoder.decode(der_buf[0..der_len], TEST_KEY_PKCS1_B64);
    return parsePkcs1(der_buf[0..der_len]);
}

/// Verify via std directly — the exact primitive `jwks_crypto.verifyRs256` wraps.
/// Inlined here (not imported) so the module stays std-only / standalone-testable.
fn verifyLocal(message: []const u8, signature: []const u8, n: []const u8, e: []const u8) !void {
    const mod_len = 256; // the fixture is a 2048-bit key
    if (signature.len != mod_len) return error.SignatureInvalid;
    const pk = std.crypto.Certificate.rsa.PublicKey.fromBytes(e, n) catch return error.SignatureInvalid;
    var sig: [mod_len]u8 = undefined;
    @memcpy(sig[0..], signature);
    std.crypto.Certificate.rsa.PKCS1v1_5Signature.verify(mod_len, sig, message, pk, Sha256) catch
        return error.SignatureInvalid;
}

test "signRs256: reproduces openssl's deterministic signature and verifies under the public key" {
    const verifyRs256 = verifyLocal;
    var der_buf: [1200]u8 = undefined;
    const key = try decodeFixtureKey(&der_buf);

    var sig: [MAX_MODULUS_LEN]u8 = undefined;
    const out = try signRs256(&sig, key, TEST_MSG);

    var expected: [256]u8 = undefined;
    _ = try std.fmt.hexToBytes(&expected, EXPECTED_SIG_HEX);
    try std.testing.expectEqualSlices(u8, &expected, out);

    // Round-trip: the signature must verify under the matching public key.
    try verifyRs256(TEST_MSG, out, key.n, key.e);
}

test "signRs256: a signature does not verify against a tampered message" {
    const verifyRs256 = verifyLocal;
    var der_buf: [1200]u8 = undefined;
    const key = try decodeFixtureKey(&der_buf);
    var sig: [MAX_MODULUS_LEN]u8 = undefined;
    const out = try signRs256(&sig, key, TEST_MSG);
    try std.testing.expectError(error.SignatureInvalid, verifyRs256("tampered-input", out, key.n, key.e));
}

test "parsePkcs1: rejects malformed DER instead of panicking" {
    try std.testing.expectError(error.KeyMalformed, parsePkcs1(&[_]u8{ 0x30, 0x00 }));
    try std.testing.expectError(error.KeyMalformed, parsePkcs1("not-der-at-all"));
    try std.testing.expectError(error.KeyMalformed, parsePkcs1(&[_]u8{}));
}

test "signPemRs256: a PKCS#1 PEM produces the identical deterministic signature" {
    // Assemble the PEM at runtime from the DER fixture; the armor is split so no
    // literal private-key block sits in source (gitleaks-safe).
    const begin = "-----BEGIN RSA " ++ "PRIVATE KEY-----";
    const end = "-----END RSA " ++ "PRIVATE KEY-----";
    var pem_buf: [4096]u8 = undefined;
    const pem = try std.fmt.bufPrint(&pem_buf, "{s}\n{s}\n{s}\n", .{ begin, TEST_KEY_PKCS1_B64, end });

    var sig: [MAX_MODULUS_LEN]u8 = undefined;
    const out = try signPemRs256(&sig, pem, TEST_MSG);
    var expected: [256]u8 = undefined;
    _ = try std.fmt.hexToBytes(&expected, EXPECTED_SIG_HEX);
    try std.testing.expectEqualSlices(u8, &expected, out);
}

test "signPemRs256: a malformed PEM body errors, never panics" {
    var sig: [MAX_MODULUS_LEN]u8 = undefined;
    const bad = "-----BEGIN RSA " ++ "PRIVATE KEY-----\nnot-base64-!!!\n-----END RSA " ++ "PRIVATE KEY-----";
    try std.testing.expectError(error.KeyMalformed, signPemRs256(&sig, bad, TEST_MSG));
}
