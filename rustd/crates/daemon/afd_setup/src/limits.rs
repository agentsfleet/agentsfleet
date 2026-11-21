//! Setup has no limits of its own, and no deployment setting feeds a setup
//! run. A setup run renews as a fleet's does: the payer's balance, checked at
//! assignment and at every renewal, and a person's Stop end a runaway, and the
//! proposal loop inside it needs no cap (decision D4, no retry cap).
//!
//! What is left here guards the setting and the limits that went.

#[expect(
    clippy::expect_used,
    reason = "a source tree that will not read fails the regression guard"
)]
mod tests {
    use std::{fs, path::Path};

    /// The names the deleted setting and limits spelled, built so this file
    /// does not spell them itself.
    fn retired() -> [String; 5] {
        [
            ["Turn", "Settings"].concat(),
            ["Turn", "Policy"].concat(),
            ["turn", "_policy"].concat(),
            ["SETUP", "_RUNTIME"].concat(),
            ["MODEL", "_CALLS_MAX"].concat(),
        ]
    }

    /// Every Rust file under `root`.
    fn sources(root: &Path) -> Vec<std::path::PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut found = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).expect("a source directory") {
                let path = entry.expect("a source entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().and_then(|v| v.to_str()) == Some("rs") {
                    found.push(path);
                }
            }
        }
        found
    }

    /// The setting that fed setup runs (`[authoring.turn]`), the 300-second
    /// runtime and the model-call price estimate are gone from this crate, from
    /// the fleet's run path and from the daemon's configuration.
    #[test]
    fn setup_has_no_limits_of_its_own() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let daemon = manifest.join("../main/src");
        let fleet = manifest.join("../afd_fleet/src");
        let mut read = 0_usize;
        for path in sources(&manifest.join("src"))
            .into_iter()
            .chain(sources(&daemon))
            .chain(sources(&fleet))
        {
            let source = fs::read_to_string(&path).expect("a Rust source");
            read += 1;
            for name in retired() {
                assert!(
                    !source.contains(&name),
                    "{} still names {name}",
                    path.display()
                );
            }
        }
        assert!(read > 0, "the guard read no source");
    }

    /// Every file under `root` whose extension is `extension`.
    fn files(root: &Path, extension: &str) -> Vec<std::path::PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut found = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).expect("a source directory") {
                let path = entry.expect("a source entry").path();
                if path.is_dir() {
                    if path.file_name().and_then(|name| name.to_str()) != Some("target") {
                        pending.push(path);
                    }
                } else if path.extension().and_then(|v| v.to_str()) == Some(extension) {
                    found.push(path);
                }
            }
        }
        found
    }

    /// The identifiers `source` spells, split on everything else.
    fn words(source: &str) -> std::collections::BTreeSet<&str> {
        source
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty())
            .collect()
    }

    /// The removed daemon-side authoring, read from the tree: no setup
    /// model frame in any Rust source, no model module in this crate, no
    /// runner authoring module, and no `[authoring.turn]` in a deployment.
    #[test]
    fn the_setup_model_path_leaves_no_trace() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let rustd = manifest.ancestors().nth(3).expect("the rustd directory");
        let frames: Vec<String> = [
            "ModelResult",
            "ActivityAck",
            "Renewed",
            "ReportAck",
            "Model",
            "Renew",
            "Report",
        ]
        .iter()
        .map(|tail| ["Authoring", tail].concat())
        .collect();
        let mut read = 0_usize;
        for root in [rustd.join("crates"), rustd.join("guffbolt")] {
            for path in files(&root, "rs") {
                let source = fs::read_to_string(&path).expect("a Rust source");
                read += 1;
                let spelled = words(&source);
                for frame in &frames {
                    assert!(
                        !spelled.contains(frame.as_str()),
                        "{} names {frame}",
                        path.display()
                    );
                }
            }
        }
        assert!(read > 0, "the sweep read no source");
        let module = ["mod ", "model;"].concat();
        let root = fs::read_to_string(manifest.join("src/lib.rs")).expect("the crate root");
        assert!(!root.contains(&module), "the crate root declares it");
        let path = ["crate::", "model::"].concat();
        for file in files(&manifest.join("src"), "rs") {
            let source = fs::read_to_string(&file).expect("a Rust source");
            assert!(!source.contains(&path), "{} reaches it", file.display());
        }
        let authoring = ["mod ", "authoring"].concat();
        for file in files(&rustd.join("crates/runner"), "rs") {
            let source = fs::read_to_string(&file).expect("a Rust source");
            assert!(
                !source.contains(&authoring),
                "{} declares it",
                file.display()
            );
        }
        let section = ["[authoring", ".turn]"].concat();
        let deploy = rustd.parent().expect("the repository root").join("deploy");
        for file in files(&deploy, "toml") {
            let source = fs::read_to_string(&file).expect("a deployment file");
            assert!(!source.contains(&section), "{} sets it", file.display());
        }
    }
}
