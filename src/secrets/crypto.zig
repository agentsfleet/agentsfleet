//! Facade: re-exports AES-256-GCM primitives and vault storage helpers.
//! KEK: 32-byte hex from ENCRYPTION_MASTER_KEY env var.
//! External importers see an unchanged public surface.

const cp = @import("crypto_primitives.zig");
const cs = @import("crypto_store.zig");

pub const AesGcm = cp.AesGcm;
pub const KEY_LEN = cp.KEY_LEN;
pub const NONCE_LEN = cp.NONCE_LEN;
pub const TAG_LEN = cp.TAG_LEN;

pub const SecretError = cp.SecretError;
pub const EncryptedBlob = cp.EncryptedBlob;

pub const loadKek = cp.loadKek;
pub const loadMasterKey = cp.loadMasterKey;
pub const loadKekByVersion = cp.loadKekByVersion;
pub const encrypt = cp.encrypt;
pub const decrypt = cp.decrypt;
pub const toFixed = cp.toFixed;

pub const store = cs.store;
pub const load = cs.load;
pub const reencryptSecret = cs.reencryptSecret;

test {
    _ = @import("crypto_primitives.zig");
    _ = @import("crypto_store.zig");
}
