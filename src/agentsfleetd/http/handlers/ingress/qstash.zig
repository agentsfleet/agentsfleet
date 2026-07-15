//! QStash schedule-fire ingress.
// QStash signature-verified — no bearer or generic webhook fallback.

const httpz = @import("httpz");
const logging = @import("log");

const cron_constants = @import("../../../cron/constants.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const FireQueue = @import("../../../cron/FireQueue.zig");
const FireService = @import("../../../cron/FireService.zig");
const FireStore = @import("../../../cron/FireStore.zig");
const QStashVerifier = @import("../../../cron/QStashVerifier.zig");
const common = @import("../common.zig");
const Hx = @import("../hx.zig").Hx;

const log = logging.scoped(.cron_ingress);
const SIGNATURE_DETAIL = "QStash delivery signature verification failed";
const DELIVERY_DETAIL = "QStash delivery headers or body are invalid";
const UNCONFIGURED_DETAIL = "QStash credentials are not configured";
const PROCESSING_DETAIL = "Failed to accept the scheduled event";
const EVENT_CRON_FIRE_ACCEPTED = "cron_fire_accepted";

pub fn innerQStashSchedule(hx: Hx, req: *httpz.Request) void {
    const credentials = hx.ctx.qstash_credentials orelse {
        hx.fail(error_codes.ERR_SCHEDULE_NOT_CONFIGURED, UNCONFIGURED_DETAIL);
        return;
    };
    var destination_buffer: [cron_constants.max_destination_url_bytes]u8 = undefined;
    const destination = cron_constants.destinationUrl(&destination_buffer, hx.ctx.api_url) catch {
        hx.fail(error_codes.ERR_SCHEDULE_NOT_CONFIGURED, UNCONFIGURED_DETAIL);
        return;
    };
    const request: FireService.Request = .{
        .signature = req.header(cron_constants.signature_header) orelse "",
        .schedule_id_header = req.header(cron_constants.schedule_id_header) orelse "",
        .message_id_header = req.header(cron_constants.message_id_header) orelse "",
        .raw_body = req.body() orelse "",
    };
    const service = FireService.init(
        FireStore.init(hx.ctx.pool),
        FireQueue.init(hx.ctx.alloc, hx.ctx.queue),
        QStashVerifier.init(destination, credentials.current_signing_key, credentials.next_signing_key),
    );
    const outcome = service.process(hx.alloc, request) catch |err| {
        reject(hx, err);
        return;
    };
    logOutcome(outcome);
    hx.ok(.ok, .{ .accepted = true });
}

fn reject(hx: Hx, err: anyerror) void {
    switch (err) {
        error.SigningKeysMissing => hx.fail(error_codes.ERR_SCHEDULE_NOT_CONFIGURED, UNCONFIGURED_DETAIL),
        error.InvalidMessageId,
        error.InvalidFireBody,
        error.ScheduleIdMismatch,
        => hx.fail(error_codes.ERR_INVALID_REQUEST, DELIVERY_DETAIL),
        error.TokenTooLarge,
        error.TokenMalformed,
        error.UnsupportedAlgorithm,
        error.SignatureInvalid,
        error.ClaimsInvalid,
        error.IssuerMismatch,
        error.SubjectMismatch,
        error.TokenExpired,
        error.TokenNotYetValid,
        error.BodyMismatch,
        => hx.fail(error_codes.ERR_SCHEDULE_SIGNATURE_INVALID, SIGNATURE_DETAIL),
        else => common.internalOperationError(hx.res, PROCESSING_DETAIL, hx.req_id),
    }
}

fn logOutcome(outcome: FireService.Outcome) void {
    switch (outcome) {
        .enqueued => log.debug(EVENT_CRON_FIRE_ACCEPTED, .{ .outcome = "enqueued" }),
        .duplicate => log.debug(EVENT_CRON_FIRE_ACCEPTED, .{ .outcome = "duplicate" }),
        .ignored => |reason| log.debug(EVENT_CRON_FIRE_ACCEPTED, .{ .outcome = "ignored", .reason = @tagName(reason) }),
    }
}
