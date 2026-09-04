use super::{
    MAX_MARKDOWN_LEN, MAX_SOURCE_REF_LEN, MAX_SUPPORT_FILE_LEN, MAX_SUPPORT_FILES,
    MAX_SUPPORT_PATH_LEN, body, classify,
};
use crate::{ImportBody, InvalidBundle, SourceKind, SupportFile};

fn upload() -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Upload,
        source_ref: "operator-upload".to_owned(),
        source_revision: None,
        skill_markdown: b"valid".to_vec(),
        trigger_markdown: None,
        support_files: Vec::new(),
    }
}

#[test]
fn each_document_bound_refuses_at_the_boundary_it_owns() {
    let mut value = upload();
    value.source_ref = "r".repeat(MAX_SOURCE_REF_LEN + 1);
    assert_eq!(body(&value), Err(InvalidBundle::SourceRefTooLong));

    value = upload();
    value.skill_markdown.clear();
    assert_eq!(body(&value), Err(InvalidBundle::MissingSkill));

    value = upload();
    value.skill_markdown = vec![b's'; MAX_MARKDOWN_LEN + 1];
    assert_eq!(body(&value), Err(InvalidBundle::SkillTooLarge));

    value = upload();
    value.trigger_markdown = Some(Vec::new());
    assert_eq!(body(&value), Err(InvalidBundle::InvalidTrigger));

    value.trigger_markdown = Some(vec![b't'; MAX_MARKDOWN_LEN + 1]);
    assert_eq!(body(&value), Err(InvalidBundle::TriggerTooLarge));
}

#[test]
fn credentials_are_refused_in_every_document_channel() {
    for mutate in [
        |body: &mut ImportBody| body.skill_markdown = b"api_key: exposed".to_vec(),
        |body: &mut ImportBody| {
            body.trigger_markdown = Some(b"client_secret: exposed".to_vec());
        },
        |body: &mut ImportBody| {
            body.support_files.push(SupportFile {
                path: "notes.txt".to_owned(),
                content: b"op://vault/item".to_vec(),
            });
        },
    ] {
        let mut value = upload();
        mutate(&mut value);
        assert_eq!(body(&value), Err(InvalidBundle::EmbeddedCredential));
    }
}

#[test]
fn support_files_enforce_count_path_individual_and_aggregate_bounds() {
    let mut value = upload();
    value.support_files = (0..=MAX_SUPPORT_FILES)
        .map(|index| SupportFile {
            path: format!("{index}.txt"),
            content: Vec::new(),
        })
        .collect();
    assert_eq!(body(&value), Err(InvalidBundle::TooManySupportFiles));

    for path in [
        String::new(),
        "SKILL.md".to_owned(),
        "a//b".to_owned(),
        "a\\b".to_owned(),
        "../outside".to_owned(),
        "p".repeat(MAX_SUPPORT_PATH_LEN + 1),
    ] {
        value = upload();
        value.support_files.push(SupportFile {
            path,
            content: Vec::new(),
        });
        assert_eq!(body(&value), Err(InvalidBundle::UnsafeSupportPath));
    }

    value = upload();
    value.support_files.push(SupportFile {
        path: "large.bin".to_owned(),
        content: vec![0; MAX_SUPPORT_FILE_LEN + 1],
    });
    assert_eq!(body(&value), Err(InvalidBundle::SupportFileTooLarge));

    value = upload();
    value.support_files = (0..5)
        .map(|index| SupportFile {
            path: format!("{index}.bin"),
            content: vec![0; MAX_SUPPORT_FILE_LEN],
        })
        .collect();
    assert_eq!(body(&value), Err(InvalidBundle::SupportFilesTooLarge));
}

/// The first-party `platform-ops` bundle, read at COMPILE time from the same
/// corpus the runtime's suites read.
///
/// Embedded rather than opened: a test that reads the corpus at runtime turns
/// a moved fixture into a panic in one case, where this turns it into a build
/// failure across every case that depends on it.
const PLATFORM_OPS_SKILL: &[u8] =
    include_bytes!("../../../../../tests/fixtures/fleetbundle/platform-ops/SKILL.md");

/// Its trigger document — the one carrying the documented credential shapes.
const PLATFORM_OPS_TRIGGER: &[u8] =
    include_bytes!("../../../../../tests/fixtures/fleetbundle/platform-ops/TRIGGER.md");

fn platform_ops() -> ImportBody {
    let mut value = upload();
    value.skill_markdown = PLATFORM_OPS_SKILL.to_vec();
    value.trigger_markdown = Some(PLATFORM_OPS_TRIGGER.to_vec());
    value
}

/// The regression this file exists for.
///
/// `platform-ops/TRIGGER.md` documents the shape of each credential it needs
/// in a YAML comment — `# github = { webhook_secret: "<base64>", ... }`. The
/// scanner matched the bare key and refused the bundle at onboard with
/// `UZ-BUNDLE-001`, which took the whole CLI acceptance lane down with it: no
/// fixture fleet installed, so every lifecycle case behind it failed too.
#[test]
fn first_party_bundles_documenting_credential_shapes_are_not_refused() {
    assert_eq!(body(&platform_ops()), Ok(()));
}

/// The other half of that fix: reading the document must not stop it refusing.
///
/// The values are deliberately mundane. The rule tests whether a mapping
/// assigns a credential key something real — it reads no entropy and no vendor
/// prefix — so a realistic-looking token would prove nothing extra and would
/// trip the repository's own secret scanner on the way past.
#[test]
fn a_credential_assigned_in_the_document_is_still_refused() {
    for leak in [
        &b"webhook_secret: hunter2"[..],
        &b"api_key: not-a-placeholder"[..],
        &b"access_token: plain-value"[..],
        &b"outer:\n  inner:\n    client_secret: buried"[..],
        &b"items:\n  - api_key: in-a-sequence"[..],
        &b"op://vault/item"[..],
        &b"-----BEGIN PRIVATE KEY-----"[..],
    ] {
        let mut value = upload();
        value.trigger_markdown = Some(leak.to_vec());
        assert_eq!(
            body(&value),
            Err(InvalidBundle::EmbeddedCredential),
            "expected a refusal for {}",
            String::from_utf8_lossy(leak)
        );
    }
}

/// What a bundle is allowed to say about the credentials it needs.
///
/// The first case is the regression itself: a COMMENT. Once the document is
/// parsed there is no comment left to match, which is why parsing fixes this
/// at the root where a cleverer byte pattern only moves the false positive.
#[test]
fn documented_credential_shapes_and_substitutions_are_admitted() {
    for benign in [
        &b"# github = { webhook_secret: \"<base64>\", api_key: \"<gh PAT>\" }\nname: x"[..],
        &b"webhook_secret: \"{{github_webhook_secret}}\""[..],
        &b"api_key: ${secrets.openai.api_key}"[..],
        &b"access_token:"[..],
        &b"client_secret: ''"[..],
        &b"api_key: \"<gh PAT>\""[..],
    ] {
        let mut value = upload();
        value.trigger_markdown = Some(benign.to_vec());
        assert_eq!(
            body(&value),
            Ok(()),
            "expected admission for {}",
            String::from_utf8_lossy(benign)
        );
    }
}

/// Puts one body out of bounds, in exactly one way.
type BreakRule = dyn Fn(&mut ImportBody);

/// The guard on the port itself.
///
/// `garde` reports a PATH and a MESSAGE, never a variant, and
/// `Error::code()` splits these variants across `UZ-REQ-002` and
/// `UZ-BUNDLE-001`. Every variant this module can produce is driven here from
/// a body that breaks exactly its rule, so a report that stopped naming one
/// fails here rather than silently answering the other status code.
#[test]
fn every_rule_this_module_owns_still_reports_its_own_variant() {
    let long_path = "p".repeat(MAX_SUPPORT_PATH_LEN + 1);
    let cases: [(InvalidBundle, &BreakRule); 9] = [
        (InvalidBundle::SourceRefTooLong, &|body: &mut ImportBody| {
            body.source_ref = "r".repeat(MAX_SOURCE_REF_LEN + 1);
        }),
        (InvalidBundle::MissingSkill, &|body: &mut ImportBody| {
            body.skill_markdown.clear();
        }),
        (InvalidBundle::SkillTooLarge, &|body: &mut ImportBody| {
            body.skill_markdown = vec![b's'; MAX_MARKDOWN_LEN + 1];
        }),
        (InvalidBundle::InvalidTrigger, &|body: &mut ImportBody| {
            body.trigger_markdown = Some(Vec::new());
        }),
        (InvalidBundle::TriggerTooLarge, &|body: &mut ImportBody| {
            body.trigger_markdown = Some(vec![b't'; MAX_MARKDOWN_LEN + 1]);
        }),
        (
            InvalidBundle::TooManySupportFiles,
            &|body: &mut ImportBody| {
                body.support_files = (0..=MAX_SUPPORT_FILES)
                    .map(|index| SupportFile {
                        path: format!("{index}.txt"),
                        content: Vec::new(),
                    })
                    .collect();
            },
        ),
        (
            InvalidBundle::UnsafeSupportPath,
            &|body: &mut ImportBody| {
                body.support_files.push(SupportFile {
                    path: "../outside".to_owned(),
                    content: Vec::new(),
                });
            },
        ),
        (
            InvalidBundle::SupportFileTooLarge,
            &|body: &mut ImportBody| {
                body.support_files.push(SupportFile {
                    path: "large.bin".to_owned(),
                    content: vec![0; MAX_SUPPORT_FILE_LEN + 1],
                });
            },
        ),
        (
            InvalidBundle::SupportFilesTooLarge,
            &|body: &mut ImportBody| {
                body.support_files = (0..5)
                    .map(|index| SupportFile {
                        path: format!("{index}.bin"),
                        content: vec![0; MAX_SUPPORT_FILE_LEN],
                    })
                    .collect();
            },
        ),
    ];

    for (expected, break_it) in cases {
        let mut value = upload();
        break_it(&mut value);
        assert_eq!(body(&value), Err(expected), "expected {expected}");
    }

    // The over-long path travels the same arm as traversal, and is driven
    // separately so the borrow of `long_path` stays local to this assertion.
    let mut value = upload();
    value.support_files.push(SupportFile {
        path: long_path,
        content: Vec::new(),
    });
    assert_eq!(body(&value), Err(InvalidBundle::UnsafeSupportPath));
}

/// A report this module has no rule for degrades to the strictest refusal.
///
/// Every custom rule names itself and every `length` bound sits on a path
/// `classify` knows, so no [`ImportBody`] reaches this arm through [`body`] —
/// it is driven directly. What it pins is the direction of the default: a
/// bound added to the model without a mapping here must surface as a refusal
/// the caller cannot mistake for an admitted file, never as a pass.
#[test]
fn a_report_under_no_known_path_is_refused_rather_than_admitted() {
    assert_eq!(
        classify("source_revision", "length is lower than 1"),
        InvalidBundle::UnsafeSupportPath
    );
    assert_eq!(
        classify("support_files[0].path", "length is greater than 160"),
        InvalidBundle::UnsafeSupportPath,
        "a support-file field that is not `content` is the path, and a path \
         finding is a path refusal"
    );
}
