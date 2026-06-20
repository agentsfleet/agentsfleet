const Template = @import("Template.zig");

const ITEMS = [_]Template{
    .{
        .id = "github-pr-reviewer",
        .name = "GitHub Pull Request reviewer",
        .description = "Reviews GitHub pull requests and posts review comments.",
        .required_credentials = &.{"github"},
        .required_tools = &.{"github_review_comment"},
        .network_hosts = &.{"api.github.com"},
    },
    .{
        .id = "zoho-recruit-outreach",
        .name = "Zoho Recruit outreach",
        .description = "Drafts candidate outreach from Zoho Recruit context.",
        .required_credentials = &.{ "zoho", "mail" },
        .required_tools = &.{"candidate_outreach"},
        .network_hosts = &.{ "recruit.zoho.com", "accounts.zoho.com" },
    },
};

pub fn all() []const Template {
    return ITEMS[0..];
}
