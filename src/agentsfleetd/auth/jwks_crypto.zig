const std = @import("std");
const VerifyError = @import("jwks_types.zig").VerifyError;

pub fn verifyRs256(message: []const u8, signature: []const u8, modulus: []const u8, exponent: []const u8) !void {
    switch (modulus.len) {
        inline 128, 256, 384, 512 => |mod_len| {
            if (signature.len != mod_len) return VerifyError.SignatureInvalid;
            const public_key = std.crypto.Certificate.rsa.PublicKey.fromBytes(exponent, modulus) catch return VerifyError.SignatureInvalid;
            var sig: [mod_len]u8 = undefined;
            @memcpy(sig[0..], signature);
            std.crypto.Certificate.rsa.PKCS1v1_5Signature.verify(mod_len, sig, message, public_key, std.crypto.hash.sha2.Sha256) catch {
                return VerifyError.SignatureInvalid;
            };
        },
        else => return VerifyError.SignatureInvalid,
    }
}
