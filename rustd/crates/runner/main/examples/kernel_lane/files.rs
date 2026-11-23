//! The file tools in a real sandbox: each works `/workspace` through the
//! executor bubblewrap serves, what it writes is the sandbox user's, and a
//! link out of the workspace is refused by the executor's handle inside it.

use afr_sandbox::{Engine, Limits, SandboxRequest};
use afr_tools::catalog::{APPLY_PATCH, FILE_EDIT, FILE_READ, FILE_WRITE, IMAGE, SHELL};
use afr_tools::{Run, ToolErrorCode};
use guffbolt::ToolOutput;
use libtest_mimic::Failed;
use serde_json::{Value, json};

use crate::lane::Lane;
use crate::run::{expect, runtime};
use crate::tools::called;

/// The argument names the calls spell.
const PATH: &str = "path";
const CONTENT: &str = "content";
const COMMAND: &str = "command";
const EDITS: &str = "edits";
const OLD_TEXT: &str = "old_text";
const NEW_TEXT: &str = "new_text";
const PATCH_FIELD: &str = "patch";
/// The file the trial works on.
const TODO: &str = "notes/todo.txt";
/// What the file holds once every tool has had its turn.
const FINAL: &str = "one\nTWO\nthree\n";
/// The image a trial writes, and where: a whole 1-by-1 grayscale PNG of
/// 67 bytes, since `image` decodes what it attaches and refuses a bare
/// signature as `not_an_image`.
const SHOT: &str = "notes/shot.png";
const WRITE_PNG: &str = concat!(
    "printf '",
    "\\211\\120\\116\\107\\015\\012\\032\\012\\000\\000\\000\\015\\111\\110\\104\\122\\000\\000\\000\\001\\000\\000\\000\\001\\010\\000\\000\\000\\000\\072\\176\\233\\125\\000",
    "\\000\\000\\012\\111\\104\\101\\124\\170\\332\\143\\140\\000\\000\\000\\002\\000\\001\\345\\047\\336\\374\\000\\000\\000\\000\\111\\105\\116\\104\\256\\102\\140\\202",
    "' > notes/shot.png",
);
/// Prints whether the file is the sandbox user's own.
const OWNED: &str = "[ \"$(stat -c %u notes/todo.txt)\" = \"$(id -u)\" ] && echo owned";
/// A link from the workspace to the sandbox's own `/etc`.
const LINK_OUT: &str = "ln -s /etc link && echo linked";
/// A patch that adds one file and rewrites one line of the trial's file.
const PATCH: &str = concat!(
    "*** Begin Patch\n",
    "*** Add File: notes/extra.txt\n",
    "+extra\n",
    "*** Update File: notes/todo.txt\n",
    "@@\n",
    "-two\n",
    "+TWO\n",
    "*** End Patch\n",
);
/// A patch that removes the file the first one added.
const DELETE_EXTRA: &str = "*** Begin Patch\n*** Delete File: notes/extra.txt\n*** End Patch\n";

/// One call, built from the answers before it.
type Step = fn(&[ToolOutput]) -> (&'static str, Value);

/// The tools the trials call: each one a run builds over the sandbox.
const NAMES: [&str; 6] = [
    SHELL.name(),
    FILE_READ.name(),
    FILE_WRITE.name(),
    FILE_EDIT.name(),
    APPLY_PATCH.name(),
    IMAGE.name(),
];

/// Makes each step's call in order, in one fresh sandbox, through the tools
/// a run builds; what each answered.
fn in_fresh_sandbox(lane: &Lane, run_id: &str, steps: &[Step]) -> Result<Vec<ToolOutput>, Failed> {
    runtime().block_on(async {
        let engine = lane.engine();
        let request = SandboxRequest::new(run_id, Limits::default());
        let sandbox = engine.prepare(request).await?;
        let run = Run::default().with_image_input(true);
        let mut outputs = Vec::with_capacity(steps.len());
        for (number, step) in (1_u64..).zip(steps) {
            let (name, arguments) = step(&outputs);
            if !NAMES.contains(&name) {
                return Err(format!("{name} is not one of the trial's tools").into());
            }
            let executor = sandbox.executor();
            outputs.push(called(executor, &run, number, name, &arguments).await?);
        }
        sandbox.destroy().await?;
        Ok(outputs)
    })
}

/// A trial that got other than `expected` answers.
fn wrong_count(expected: usize, outputs: &[ToolOutput]) -> Failed {
    format!("{expected} answers, got {outputs:?}").into()
}

/// Every answer succeeded with no code.
fn all_succeeded(outputs: &[ToolOutput]) -> Result<(), Failed> {
    for output in outputs {
        expect(
            output.error_code.is_none(),
            format!("no code, got {output:?}"),
        )?;
    }
    Ok(())
}

/// `file_edit`'s arguments on `path`, one edit for each `(old, new)` pair.
fn edits(path: &str, pairs: &[(&str, &str)]) -> Value {
    let edits: Vec<Value> = pairs
        .iter()
        .map(|(old, new)| json!({OLD_TEXT: old, NEW_TEXT: new}))
        .collect();
    json!({PATH: path, EDITS: edits})
}

/// Write, read, edit and `apply_patch` each work the workspace from inside
/// the sandbox, as its user, and a patch's removal takes away what one added.
pub(crate) fn file_tools_run_inside(lane: &Lane) -> Result<(), Failed> {
    let steps: [Step; 9] = [
        |_before| {
            (
                FILE_WRITE.name(),
                json!({PATH: TODO, CONTENT: "one\n2\n3\n"}),
            )
        },
        |_before| {
            (
                FILE_EDIT.name(),
                edits(TODO, &[("2", "two"), ("3", "three")]),
            )
        },
        |_before| (APPLY_PATCH.name(), json!({PATCH_FIELD: PATCH})),
        |_before| (FILE_READ.name(), json!({PATH: TODO})),
        |_before| (SHELL.name(), json!({COMMAND: OWNED})),
        |_before| (SHELL.name(), json!({COMMAND: WRITE_PNG})),
        |_before| (IMAGE.name(), json!({PATH: SHOT})),
        |_before| (APPLY_PATCH.name(), json!({PATCH_FIELD: DELETE_EXTRA})),
        |_before| (SHELL.name(), json!({COMMAND: "ls notes"})),
    ];
    let outputs = in_fresh_sandbox(lane, "files-inside", &steps)?;
    all_succeeded(&outputs)?;
    let [
        _write,
        _edit,
        patched,
        read,
        owned,
        _png,
        viewed,
        _delete,
        listed,
    ] = outputs.as_slice()
    else {
        return Err(wrong_count(steps.len(), &outputs));
    };
    expect(
        viewed
            .text
            .starts_with("Attached notes/shot.png (67 bytes, image/png)"),
        format!(
            "the image is read inside the sandbox, got {:?}",
            viewed.text
        ),
    )?;
    expect(
        patched
            .text
            .ends_with("Lines changed:\n\"notes/extra.txt\" +1 -0\n\"notes/todo.txt\" +1 -1"),
        format!("the patch counts its lines, got {:?}", patched.text),
    )?;
    expect(
        read.text == FINAL,
        format!("every edit landed, got {:?}", read.text),
    )?;
    expect(
        owned.text.trim() == "owned",
        format!("the file is the sandbox user's, got {:?}", owned.text),
    )?;
    expect(
        listed.text.trim() == "shot.png\ntodo.txt",
        format!("the added file is gone again, got {:?}", listed.text),
    )
}

/// A link to the sandbox's own `/etc` is refused by the executor's handle,
/// and a path that climbs out by name never reaches it.
pub(crate) fn file_tools_refuse_link_out(lane: &Lane) -> Result<(), Failed> {
    let steps: [Step; 4] = [
        |_before| (SHELL.name(), json!({COMMAND: LINK_OUT})),
        |_before| (FILE_READ.name(), json!({PATH: "link/passwd"})),
        |_before| {
            (
                FILE_WRITE.name(),
                json!({PATH: "link/planted", CONTENT: "x"}),
            )
        },
        |_before| (FILE_EDIT.name(), edits("../etc/passwd", &[("root", "x")])),
    ];
    let outputs = in_fresh_sandbox(lane, "files-link-out", &steps)?;
    let [linked, read, written, edited] = outputs.as_slice() else {
        return Err(wrong_count(steps.len(), &outputs));
    };
    expect(
        linked.text.trim() == "linked",
        format!("the link was made, got {linked:?}"),
    )?;
    for refused in [read, written, edited] {
        expect(
            refused.error_code == Some(ToolErrorCode::PathNotAllowed.into()),
            format!("path_not_allowed, got {refused:?}"),
        )?;
    }
    Ok(())
}
