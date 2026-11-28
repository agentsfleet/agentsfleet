//! Failures of the run's web tools.

use super::ErrorCode;

/// Fleet web access is restricted.
pub const WEB_NETWORK_RESTRICTED: ErrorCode = ErrorCode::declare("UZ-WEB-001");

/// Web vendor unavailable.
pub const WEB_VENDOR_UNCONFIGURED: ErrorCode = ErrorCode::declare("UZ-WEB-002");

/// Web vendors are cooling down.
pub const WEB_VENDORS_COOLING: ErrorCode = ErrorCode::declare("UZ-WEB-003");

/// Web search failed.
pub const WEB_VENDORS_FAILED: ErrorCode = ErrorCode::declare("UZ-WEB-004");

/// Page URL refused.
pub const WEB_INVALID_URL: ErrorCode = ErrorCode::declare("UZ-WEB-005");

/// Web reservation unavailable.
pub const WEB_RESERVATION_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-WEB-006");

/// Page unavailable.
pub const WEB_PAGE_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-WEB-007");

/// Invalid page reader response.
pub const WEB_EXTRACT_INVALID_RESPONSE: ErrorCode = ErrorCode::declare("UZ-WEB-008");

/// Page reader unavailable.
pub const WEB_EXTRACT_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-WEB-009");

/// Web call deadline expired.
pub const WEB_DEADLINE_EXCEEDED: ErrorCode = ErrorCode::declare("UZ-WEB-010");
