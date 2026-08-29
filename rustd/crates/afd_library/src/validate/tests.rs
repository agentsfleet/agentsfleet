use super::{
    MAX_MARKDOWN_LEN, MAX_SOURCE_REF_LEN, MAX_SUPPORT_FILE_LEN, MAX_SUPPORT_FILES,
    MAX_SUPPORT_PATH_LEN, body,
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
