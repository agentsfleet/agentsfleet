// The chat composer's names and copy, and the operation-id shape, as the steer
// suites assert them. No test-runner imports: component, hook and ledger suites
// share it without pulling in the fleet-thread harness's module mocks.

export const COMPOSER_NAME = "Message this fleet…";
export const SEND_FAILED_TEXT = "Message not sent.";
export const SEND_UNCONFIRMED_TEXT = "Couldn't confirm this message was sent.";
export const TOO_LONG_TEXT = "Messages can be at most 8,192 bytes.";
export const RESEND_LABEL = "Resend";
export const SEND_LABEL = "Send";
export const SIGN_IN_LABEL = "Sign in";
export const DISMISS_LABEL = "Dismiss";
export const NOTICES_LABEL = "Unsent messages";
export const UUID_V7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
