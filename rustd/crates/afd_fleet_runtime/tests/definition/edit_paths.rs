//! Array edits and failed traversal retain the accepted definition atomically.
use super::{parse, value};
use afd_fleet_runtime::definition::{ConfigOp, EditBatch};
use serde_json::json;

#[test]
fn tool_replacement_removal_and_append_preserve_the_requested_order() {
    let original = parse(&value());
    let changed = original
        .edit(EditBatch {
            config_ops: vec![
                ConfigOp::Replace {
                    path: "/config/x-agentsfleet/tools/0".into(),
                    value: json!("read_logs"),
                },
                ConfigOp::Remove {
                    path: "/config/x-agentsfleet/tools/1".into(),
                },
                ConfigOp::Add {
                    path: "/config/x-agentsfleet/tools/-".into(),
                    value: json!("shell"),
                },
            ],
            ..EditBatch::default()
        })
        .expect("ordered edits");
    assert_eq!(
        changed.runtime().tools(),
        &[Box::<str>::from("read_logs"), Box::<str>::from("shell")]
    );
    assert_eq!(original, parse(&value()));
}

#[test]
fn out_of_bounds_array_edits_fail_atomically() {
    assert_refusals([
        (
            ConfigOp::Replace {
                path: "/config/x-agentsfleet/tools/9".into(),
                value: json!("shell"),
            },
            "outside its bounds",
        ),
        (
            ConfigOp::Remove {
                path: "/config/x-agentsfleet/tools/9".into(),
            },
            "outside its bounds",
        ),
        (
            ConfigOp::Add {
                path: "/config/x-agentsfleet/tools/9".into(),
                value: json!("shell"),
            },
            "outside its bounds",
        ),
    ]);
}

#[test]
fn absent_members_and_scalar_parents_fail_atomically() {
    assert_refusals([
        (
            ConfigOp::Remove {
                path: "/config/x-agentsfleet/absent".into(),
            },
            "removed member is absent",
        ),
        (
            ConfigOp::Replace {
                path: "/config/x-agentsfleet/absent".into(),
                value: json!(0),
            },
            "replacement member is absent",
        ),
        (
            ConfigOp::Add {
                path: "/config/x-agentsfleet/tools/0/member".into(),
                value: json!(0),
            },
            "parent must be an object or array",
        ),
        (
            ConfigOp::Add {
                path: "/config/x-agentsfleet/tools/0/member/child".into(),
                value: json!(0),
            },
            "parent does not exist",
        ),
        (
            ConfigOp::Remove {
                path: "/config/x-agentsfleet/a~1b~0c".into(),
            },
            "removed member is absent",
        ),
    ]);
}

fn assert_refusals(cases: impl IntoIterator<Item = (ConfigOp, &'static str)>) {
    let original = parse(&value());
    for (operation, expected) in cases {
        let failure = original
            .edit(EditBatch {
                config_ops: vec![
                    ConfigOp::Replace {
                        path: "/description".into(),
                        value: json!("Changed first"),
                    },
                    operation,
                ],
                ..EditBatch::default()
            })
            .expect_err("refused batch");
        assert_eq!(
            failure.code(),
            afd_core::error_code::AGENTSFLEET_INVALID_CONFIG
        );
        assert!(failure.to_string().contains(expected), "{failure}");
        assert_eq!(original, parse(&value()));
    }
}
