//! The parser's own cases: Definitions that are not fleets, kept beside the
//! tests that read them.

/// The smallest Definition the parser accepts.
pub(crate) const MINIMAL: &str = r#"{
  "schema_version": 1,
  "description": "The fewest fields a setup needs.",
  "instructions_md": "You are a minimal agent. Instructions go here.",
  "config": {
    "name": "minimal-setup",
    "x-agentsfleet": {
      "triggers": [
        {
          "type": "cron",
          "schedule": "0 9 * * *"
        }
      ],
      "tools": [
        "agentmail"
      ],
      "budget": {
        "daily_nanos": "1000000000"
      }
    }
  }
}"#;

/// Every field set: a cron with no timezone and a webhook with no repositories.
pub(crate) const FULL: &str = r#"{
  "schema_version": 1,
  "description": "Every optional configuration block populated.",
  "instructions_md": "You are a full-fields agent. The configuration carries every optional\nblock: a signed webhook trigger, credentials, a network allowlist and a\nmonthly ceiling.",
  "config": {
    "name": "full-setup",
    "x-agentsfleet": {
      "triggers": [
        {
          "type": "webhook",
          "source": "github",
          "credential_name": "github_secret"
        }
      ],
      "tools": [
        "agentmail",
        "slack",
        "github"
      ],
      "credentials": [
        "github_token",
        "slack_bot_token"
      ],
      "network": {
        "allow": [
          "api.github.com",
          "slack.com"
        ]
      },
      "budget": {
        "daily_nanos": "1000000000",
        "monthly_nanos": "8000000000"
      }
    }
  }
}"#;

/// A templated runtime model and every context setting, `tool_window: "auto"` among them.
pub(crate) const WITH_MODEL_AND_CONTEXT: &str = r#"{
  "schema_version": 1,
  "description": "A runtime model and every context knob.",
  "instructions_md": "",
  "config": {
    "name": "with-model-and-context",
    "x-agentsfleet": {
      "model": "{{model}}",
      "context": {
        "context_cap_tokens": "{{context_cap_tokens}}",
        "tool_window": "auto",
        "memory_checkpoint_every": 5,
        "stage_chunk_threshold": 0.75
      },
      "triggers": [
        {
          "type": "webhook",
          "source": "github",
          "credential_name": "github_secret"
        }
      ],
      "tools": [
        "http_request"
      ],
      "budget": {
        "daily_nanos": "1000000000"
      }
    }
  }
}"#;

/// Refused: no `name`.
pub(crate) const MISSING_NAME: &str = r#"{
  "schema_version": 1,
  "description": "A configuration without its required name.",
  "instructions_md": "The configuration name is required.",
  "config": {
    "x-agentsfleet": {
      "triggers": [],
      "tools": [],
      "budget": {
        "daily_nanos": "1000000000"
      }
    }
  }
}"#;

/// Refused: a runtime key outside `x-agentsfleet`.
pub(crate) const RUNTIME_AT_TOP_LEVEL: &str = r#"{
  "schema_version": 1,
  "description": "A refusal fixture.",
  "instructions_md": "Runtime keys belong under x-agentsfleet.",
  "config": {
    "name": "misplaced-runtime",
    "trigger": {
      "type": "cron",
      "schedule": "0 9 * * *"
    },
    "tools": [
      "agentmail"
    ],
    "budget": {
      "daily_nanos": "1000000000"
    }
  }
}"#;

/// Refused: a misspelt `x-agentsfleet` key.
pub(crate) const UNKNOWN_RUNTIME_KEY: &str = r#"{
  "schema_version": 1,
  "description": "A refusal fixture.",
  "instructions_md": "`contxt` is not a runtime key.",
  "config": {
    "name": "typo-setup",
    "x-agentsfleet": {
      "triggers": [
        {
          "type": "cron",
          "schedule": "0 9 * * *"
        }
      ],
      "tools": [
        "agentmail"
      ],
      "budget": {
        "daily_nanos": "1000000000"
      },
      "contxt": {
        "foo": "bar"
      }
    }
  }
}"#;
