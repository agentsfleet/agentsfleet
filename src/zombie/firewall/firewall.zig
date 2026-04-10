//! AI Firewall orchestrator — combines domain allowlist, endpoint policy,
//! prompt injection detection, and content scanning into a single inspection
//! pipeline. Every decision is emitted as a FirewallEvent to the activity stream.
//!
//! Usage:
//!   const fw = Firewall.init(allowed_domains, endpoint_rules);
//!   const decision = fw.inspectRequest(request);
//!   // ... if allowed, execute the tool ...
//!   const scan = fw.scanResponseBody(response_body, credentials);

const std = @import("std");
const domain_policy = @import("domain_policy.zig");
const endpoint_policy = @import("endpoint_policy.zig");
const injection_detector = @import("injection_detector.zig");
const content_scanner = @import("content_scanner.zig");

pub const FirewallDecision = domain_policy.FirewallDecision;
pub const EndpointRule = endpoint_policy.EndpointRule;
pub const InjectionResult = injection_detector.InjectionResult;
pub const ScanResult = content_scanner.ScanResult;

// Re-export sub-module parse/free for callers
pub const parseEndpointRules = endpoint_policy.parseEndpointRules;
pub const freeRules = endpoint_policy.freeRules;

// ── Event types for activity stream ────────────────────────────────────────

pub const EVT_REQUEST_ALLOWED = "firewall_request_allowed";
pub const EVT_REQUEST_BLOCKED = "firewall_request_blocked";
pub const EVT_INJECTION_DETECTED = "firewall_injection_detected";
pub const EVT_CONTENT_FLAGGED = "firewall_content_flagged";
pub const EVT_APPROVAL_TRIGGERED = "firewall_approval_triggered";

// ── Outbound request descriptor ────────────────────────────────────────────

pub const OutboundRequest = struct {
    tool: []const u8,
    method: []const u8,
    domain: []const u8,
    path: []const u8,
    body: ?[]const u8,
};

// ── Firewall event (structured for activity stream detail JSON) ────────────

pub const FirewallEvent = struct {
    event_type: []const u8,
    tool: []const u8,
    domain: []const u8,
    path: []const u8,
    decision: []const u8,
    reason: []const u8,
};

// ── Orchestrator ───────────────────────────────────────────────────────────

pub const Firewall = struct {
    allowed_domains: []const []const u8,
    endpoint_rules: []const EndpointRule,

    pub fn init(
        allowed_domains: []const []const u8,
        endpoint_rules: []const EndpointRule,
    ) Firewall {
        return .{
            .allowed_domains = allowed_domains,
            .endpoint_rules = endpoint_rules,
        };
    }

    /// Inspect an outbound request through all layers.
    /// Order: endpoint policy → domain allowlist → injection detection.
    /// Returns the first non-allow decision, or allow if all pass.
    pub fn inspectRequest(self: *const Firewall, request: OutboundRequest) FirewallDecision {
        // Layer 1: Endpoint policy (most specific, takes precedence)
        const ep_decision = endpoint_policy.checkEndpoint(
            self.endpoint_rules,
            request.method,
            request.domain,
            request.path,
        );
        switch (ep_decision) {
            .block, .requires_approval => return ep_decision,
            .allow => {},
        }

        // Layer 2: Domain allowlist
        const domain_decision = domain_policy.checkDomain(
            self.allowed_domains,
            request.domain,
        );
        switch (domain_decision) {
            .block => return domain_decision,
            else => {},
        }

        // Layer 3: Prompt injection detection on request body
        if (request.body) |body| {
            const inj = injection_detector.scanRequestBody(body);
            switch (inj) {
                .detected => return .{ .block = .{ .reason = "Prompt injection pattern detected in request body. Request blocked." } },
                .clean => {},
            }
        }

        return .{ .allow = {} };
    }

    /// Scan a response body for sensitive content.
    pub fn scanResponseBody(
        _: *const Firewall,
        body: []const u8,
        credentials: []const []const u8,
    ) ScanResult {
        return content_scanner.scanResponse(body, credentials);
    }

    /// Build the event_type string for a given decision.
    pub fn eventTypeForDecision(decision: FirewallDecision) []const u8 {
        return switch (decision) {
            .allow => EVT_REQUEST_ALLOWED,
            .block => EVT_REQUEST_BLOCKED,
            .requires_approval => EVT_APPROVAL_TRIGGERED,
        };
    }

    /// Build the event_type string for a scan result.
    pub fn eventTypeForScan(result: ScanResult) ?[]const u8 {
        return switch (result) {
            .flagged => EVT_CONTENT_FLAGGED,
            .clean, .truncated => null,
        };
    }
};

test {
    _ = @import("firewall_test.zig");
}
