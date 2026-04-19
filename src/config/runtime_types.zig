// Runtime config error types.
//
// Pure error enum. Imported by the loader, validators, and the printer —
// keeping it in its own file prevents an import cycle between env-parse
// helpers and validation helpers, both of which produce these errors.

pub const ValidationError = error{
    MissingApiKey,
    InvalidApiKeyList,
    MissingOidcJwksUrl,
    InvalidOidcProvider,
    MissingEncryptionMasterKey,
    InvalidEncryptionMasterKey,
    InvalidPort,
    InvalidApiHttpThreads,
    InvalidApiHttpWorkers,
    InvalidApiMaxClients,
    InvalidApiMaxInFlightRequests,
    InvalidReadyMaxQueueDepth,
    InvalidReadyMaxQueueAgeMs,
    InvalidKekVersion,
    MissingEncryptionMasterKeyV2,
    InvalidEncryptionMasterKeyV2,
};
