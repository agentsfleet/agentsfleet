//! The file tools in a real sandbox: each works `/workspace` through the
//! executor bubblewrap serves, what it writes is the sandbox user's, and a
//! link out of the workspace is refused by the executor's handle inside it.

use std::sync::Arc;

use afr_egress::testing::RecordingTransport;
use afr_sandbox::{Engine, Limits, SandboxRequest};
use afr_tools::catalog::{
    APPLY_PATCH, FILE_APPEND, FILE_DELETE, FILE_EDIT, FILE_EDIT_HASHED, FILE_READ,
    FILE_READ_HASHED, FILE_WRITE, IMAGE, SHELL,
};
use afr_tools::{Catalog, Lease, ToolContext, ToolErrorCode, ToolOutput};
use libtest_mimic::Failed;
use serde_json::{Value, json};

use crate::lane::Lane;
use crate::run::{expect, runtime};

/// The argument names the calls spell.
const PATH: &str = "path";
const CONTENT: &str = "content";
const COMMAND: &str = "command";
const OLD_TEXT: &str = "old_text";
const NEW_TEXT: &str = "new_text";
const TARGET: &str = "target";
const PATCH_FIELD: &str = "patch";
/// The file the trial works on.
const TODO: &str = "notes/todo.txt";
/// The line two tools in turn write as the file's second.
const TWO: &str = "two";
/// What the file holds once every tool has had its turn.
const FINAL: &str = "one\nTWO\nthree\n";
/// The image a trial writes, eight bytes of PNG, and where.
const SHOT: &str = "notes/shot.png";
const WRITE_PNG: &str = "printf '\\211PNG\\r\\n\\032\\n' > notes/shot.png";
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

/// One call, built from the answers before it.
type Step = fn(&[ToolOutput]) -> (&'static str, Value);

/// Makes each step's call in order, in one fresh sandbox, through the
/// handlers the runner's catalog hosts; what each answered.
fn in_fresh_sandbox(
    lane: &Lane,
    lease_id: &str,
    steps: &[Step],
) -> Result<Vec<ToolOutput>, Failed> {
    runtime().block_on(async {
        let (transport, _sent) = RecordingTransport::replying(200, "");
        let catalog = Catalog::hosted(Arc::new(transport));
        let names = [
            SHELL.name(),
            FILE_READ.name(),
            FILE_READ_HASHED.name(),
            FILE_WRITE.name(),
            FILE_APPEND.name(),
            FILE_DELETE.name(),
            FILE_EDIT.name(),
            FILE_EDIT_HASHED.name(),
            APPLY_PATCH.name(),
            IMAGE.name(),
        ];
        let selection = catalog.select(&names)?;
        let engine = lane.engine();
        let request = SandboxRequest::new(lease_id, Limits::default());
        let sandbox = engine.prepare(request).await?;
        let lease = Lease::default().with_image_input(true);
        let mut outputs = Vec::with_capacity(steps.len());
        for step in steps {
            let (name, arguments) = step(&outputs);
            let tool = selection.tool(name).ok_or("the runner hosts it")?;
            let context = ToolContext {
                executor: Some(sandbox.executor()),
                lease: &lease,
            };
            outputs.push(tool.call(&arguments, context).await);
        }
        sandbox.destroy().await?;
        Ok(outputs)
    })
}

/// The tag of line `number` in the last `file_read_hashed` answer.
fn tag_from(answers: &[ToolOutput], number: usize) -> String {
    answers
        .last()
        .and_then(|read| read.text.lines().nth(number - 1))
        .and_then(|line| line.split('|').next())
        .unwrap_or_default()
        .to_owned()
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

/// Write, append, read, edit, the hashed pair and `apply_patch` each work
/// the workspace from inside the sandbox, as its user, and a delete removes
/// what they made.
pub(crate) fn file_tools_run_inside(lane: &Lane) -> Result<(), Failed> {
    let steps: [Step; 12] = [
        |_before| (FILE_WRITE.name(), json!({PATH: TODO, CONTENT: "one\n"})),
        |_before| {
            (
                FILE_APPEND.name(),
                json!({PATH: TODO, CONTENT: "2\nthree\n"}),
            )
        },
        |_before| {
            (
                FILE_EDIT.name(),
                json!({PATH: TODO, OLD_TEXT: "2", NEW_TEXT: TWO}),
            )
        },
        |_before| (FILE_READ_HASHED.name(), json!({PATH: TODO})),
        |before| {
            let tag = tag_from(before, 2);
            (
                FILE_EDIT_HASHED.name(),
                json!({PATH: TODO, TARGET: tag, NEW_TEXT: TWO}),
            )
        },
        |_before| (APPLY_PATCH.name(), json!({PATCH_FIELD: PATCH})),
        |_before| (FILE_READ.name(), json!({PATH: TODO})),
        |_before| (SHELL.name(), json!({COMMAND: OWNED})),
        |_before| (SHELL.name(), json!({COMMAND: WRITE_PNG})),
        |_before| (IMAGE.name(), json!({PATH: SHOT})),
        |_before| (FILE_DELETE.name(), json!({PATH: "notes/extra.txt"})),
        |_before| (SHELL.name(), json!({COMMAND: "ls notes"})),
    ];
    let outputs = in_fresh_sandbox(lane, "files-inside", &steps)?;
    all_succeeded(&outputs)?;
    let [
        _write,
        _append,
        _edit,
        _hashed,
        _rehashed,
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
            .starts_with("Attached notes/shot.png (8 bytes, image/png)"),
        format!(
            "the image is read inside the sandbox, got {:?}",
            viewed.text
        ),
    )?;
    expect(
        patched.text.ends_with("+2 \u{2212}1"),
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
        |_before| (FILE_DELETE.name(), json!({PATH: "../etc/passwd"})),
    ];
    let outputs = in_fresh_sandbox(lane, "files-link-out", &steps)?;
    let [linked, read, written, deleted] = outputs.as_slice() else {
        return Err(wrong_count(steps.len(), &outputs));
    };
    expect(
        linked.text.trim() == "linked",
        format!("the link was made, got {linked:?}"),
    )?;
    for refused in [read, written, deleted] {
        expect(
            refused.error_code == Some(ToolErrorCode::PathNotAllowed),
            format!("path_not_allowed, got {refused:?}"),
        )?;
    }
    Ok(())
}
