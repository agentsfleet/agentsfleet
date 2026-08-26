//! The half of an approval card a human may read as fact.
//!
//! # The split is a trust boundary, and here it is a TYPE
//!
//! A card carries two kinds of statement:
//!
//! | half | author | a human may read it as |
//! |---|---|---|
//! | [`Stated`] | the daemon and the workspace | fact |
//! | [`Claim`](super::Claim) | a language model | an attributed claim |
//!
//! `approval_gate_detail.zig` holds both in ONE flat struct and keeps them
//! apart with a comment plus a naming convention. That holds exactly as long as
//! every future reader honours it — and the renderer is a different file, in a
//! different milestone, written by someone who did not read this one. Here they
//! are separate types a renderer receives separately, so attributing the
//! model's half is not a discipline it can forget.
//!
//! Every field below is either derived by the daemon from the delivery envelope
//! or authored by whoever configured the fleet. Nothing here passed through a
//! model, which is what makes the card trustworthy at all.

use afd_fleet_runtime::config::{GateRule, RepositoryBinding};

/// `core.fleet_approval_gates.gate_kind` for the unconditional write-fleet
/// park.
///
/// Deliberately NOT a gate rule. Rules ride `config_json`, which a PATCH can
/// reach under the same `fleet:write` scope that wakes the fleet — and
/// [`Decision::AutoApprove`](super::Decision::AutoApprove) is their no-match
/// fallthrough, so an emptied `rules` list would release every action.
pub const KIND_REPOSITORY_WRITE: &str = "repository_write";

/// One approval funds this many write-credential requests.
///
/// Requests spend before vault or provider access, including cached and failed
/// mints — so the ceiling bounds attempts, not successes.
pub const REPOSITORY_WRITE_SPEND_CEILING: i64 = 32;

/// The write-kind card's blast radius.
///
/// The Zig derives this from [`REPOSITORY_WRITE_SPEND_CEILING`] with
/// `comptimePrint`. Rust has no const formatter in this workspace's dependency
/// set, so the number is written out — and
/// `the_write_kind_radius_states_its_own_ceiling` is what keeps the sentence
/// and the ceiling from drifting, which is the property the derivation bought.
pub const RADIUS_REPOSITORY_WRITE: &str = "up to 32 write-credential requests, \
one branch, and one draft Pull Request in the bound repository";

/// The trustworthy half of one approval card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stated<'a> {
    /// The event's type. `tool_name` on the row.
    pub tool: &'a str,
    /// Who raised it. `action_name` on the row.
    pub action: &'a str,
    /// The event's identifier, which is what `params_summary` carries.
    pub summary: &'a str,
    /// What kind of decision the human is being asked for.
    ///
    /// Empty renders as nothing rather than as a reassuring default.
    pub kind: &'a str,
    /// How far a yes reaches. Empty renders as nothing.
    pub radius: &'a str,
    /// The fleet's repository egress binding as the DAEMON holds it — never as
    /// a model described it.
    ///
    /// The one decision-relevant fact the platform can vouch for, because it is
    /// the same value the write mint scopes its token by: whatever the model
    /// claims, the run cannot reach outside this list. Without it the card's
    /// trustworthy half named no repository and no commit — `tool`, `action`
    /// and `summary` carry event type, actor and event id, so every
    /// decision-relevant word a human read came from the model's half. The
    /// commit stays unverifiable, because approval releases a bounded run
    /// rather than specific bytes; the blast radius no longer does.
    ///
    /// `None` when the fleet declares none, which fails the mint closed, so
    /// there is no reach to state.
    pub binding: Option<&'a RepositoryBinding>,
    /// How many spending requests one yes funds, for a bounded approval.
    pub spend_ceiling: Option<i64>,
    /// How long the question stands before it lapses, in milliseconds.
    pub timeout_ms: i64,
}

impl<'a> Stated<'a> {
    /// The daemon-derived half, before a rule or a kind is stamped on it.
    ///
    /// `kind` and `radius` open empty because their source differs by path: the
    /// rules path takes both from the matched rule ([`Stated::under`]) and the
    /// write-kind path stamps the daemon's own ([`Stated::write_kind`]). A
    /// card reaching a human with neither would be a card that says what is
    /// being asked but not what kind of question it is.
    #[must_use]
    pub const fn of(
        tool: &'a str,
        action: &'a str,
        summary: &'a str,
        binding: Option<&'a RepositoryBinding>,
        timeout_ms: i64,
    ) -> Self {
        Self {
            tool,
            action,
            summary,
            kind: "",
            radius: "",
            binding,
            spend_ceiling: None,
            timeout_ms,
        }
    }

    /// The workspace copy `rule` authored for this decision.
    ///
    /// Taken from the rule that MATCHED rather than looked up again, which is
    /// what makes it impossible for the card to describe a different rule from
    /// the one that fired — see [`super::match_rule`].
    #[must_use]
    pub fn under(mut self, rule: &'a GateRule) -> Self {
        self.kind = &rule.gate_kind;
        self.radius = &rule.blast_radius;
        self
    }

    /// The daemon's own copy for the unconditional write-fleet park.
    ///
    /// No rule carries workspace copy on this path, so the kind, the radius and
    /// the ceiling are constants — and the caller supplies a default timeout
    /// rather than a policy value a PATCH could stretch.
    #[must_use]
    pub const fn write_kind(mut self) -> Self {
        self.kind = KIND_REPOSITORY_WRITE;
        self.radius = RADIUS_REPOSITORY_WRITE;
        self.spend_ceiling = Some(REPOSITORY_WRITE_SPEND_CEILING);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KIND_REPOSITORY_WRITE, RADIUS_REPOSITORY_WRITE, REPOSITORY_WRITE_SPEND_CEILING, Stated,
    };
    use afd_fleet_runtime::config::{Behavior, GateRule};

    fn rule(kind: &str, radius: &str) -> GateRule {
        GateRule {
            tool: "*".into(),
            action: "*".into(),
            condition: None,
            behavior: Behavior::Approve,
            gate_kind: kind.into(),
            blast_radius: radius.into(),
        }
    }

    #[test]
    fn the_daemon_derived_facts_are_always_present() {
        // They are what a model cannot forge, so a blank one here is a failure
        // rather than a display default.
        let stated = Stated::of("chat", "steer:user_42", "evt-1", None, 900_000);

        assert_eq!(stated.tool, "chat");
        assert_eq!(stated.action, "steer:user_42");
        assert_eq!(stated.summary, "evt-1");
        assert_eq!(stated.timeout_ms, 900_000);
    }

    #[test]
    fn a_matched_rule_supplies_the_workspace_copy() {
        let matched = rule("repair", "one draft Pull Request");
        let stated = Stated::of("chat", "steer:user_42", "evt-1", None, 900_000).under(&matched);

        assert_eq!(stated.kind, "repair");
        assert_eq!(stated.radius, "one draft Pull Request");
        // A rules-path card funds no spending: the ceiling belongs to the
        // write-kind park, which is the only approval requests draw against.
        assert_eq!(stated.spend_ceiling, None);
    }

    #[test]
    fn an_omitted_blast_radius_stays_empty_rather_than_inventing_a_reassuring_one() {
        let matched = rule("repair", "");
        let stated = Stated::of("chat", "steer:user_42", "evt-1", None, 1).under(&matched);

        assert_eq!(stated.radius, "");
    }

    #[test]
    fn the_write_kind_stamp_replaces_the_workspace_copy_entirely() {
        // The write-kind park runs where no rule matched, so nothing a fleet
        // author wrote may reach this card — including a `gate_kind` chosen to
        // look like the daemon's own.
        let misleading = rule("routine", "nothing at all");
        let stated = Stated::of("chat", "steer:user_42", "evt-1", None, 1)
            .under(&misleading)
            .write_kind();

        assert_eq!(stated.kind, KIND_REPOSITORY_WRITE);
        assert_eq!(stated.radius, RADIUS_REPOSITORY_WRITE);
        assert_eq!(stated.spend_ceiling, Some(REPOSITORY_WRITE_SPEND_CEILING));
    }

    #[test]
    fn the_write_kind_radius_states_its_own_ceiling() {
        // What `comptimePrint` bought upstream, bought here by a test: the
        // sentence a human reads and the number the mint enforces are one
        // value, so a changed ceiling cannot leave the card promising the old
        // one.
        assert!(
            RADIUS_REPOSITORY_WRITE.contains(&REPOSITORY_WRITE_SPEND_CEILING.to_string()),
            "{RADIUS_REPOSITORY_WRITE}"
        );
    }
}
