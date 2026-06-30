//! Shared offline auth fixtures for the integration suite — ONE keypair,
//! issuer, and audience for every DB-backed integration test, so the JWKS can't
//! drift per-file. Tokens themselves stay per-file (each carries its own
//! `sub`/`tenant_id`); only the verifying key + issuer + audience live here.
//!
//! Regenerate with: node scripts/regen-scope-jwts.mjs --apply

pub const ISSUER = "https://clerk.test.agentsfleet.net";
pub const AUDIENCE = "https://api.agentsfleet.net";
pub const JWKS =
    \\{"keys":[{"kty":"RSA","n":"tqDAE7F_WbDRChUyMkFTmHO55CRsLtiUoJ3sr85_EiV_zm7yoaXbYNupWRsy8O1GrBsN3dhbIPXNScI8FVy-trloXnKqN6Z263HDKkuMzsbkX5lWClOBMffAI_fzxrAsNYDfnnKRRjkeK1FP6TTuv353zLpezIywk60luD6y12rRLzbbqtS5b7Fo_7A4TSuVVljnYDq_ZhoRMdDh_QkCApOp7zcWqsWPH5NmXXXv684O7OvAkkpVuUhKvBMO1diU03z_tK-E6iuleYVMBayh9bZ5FA2sHucTqfaxyimEKbojHZMKMvupZkG5etCznrPcJ5Ed_trm-nHPKN66WWuTeQ","e":"AQAB","kid":"test-kid-static","use":"sig","alg":"RS256"}]}
;
