"""Verify the retained toolbox signature against the runner's embedded key."""

from pathlib import Path
import re
import subprocess

from .model import toolbox_digest


def verify(root: Path) -> None:
    digest = toolbox_digest(root)
    source = Path("rustd/crates/afr_sandbox/src/toolbox/manifest/mod.rs").read_text(encoding="utf-8")
    key = re.search(r"-----BEGIN PUBLIC KEY-----.*?-----END PUBLIC KEY-----", source, re.DOTALL)
    public = (root / "toolbox" / "release.pub").read_text(encoding="utf-8")
    if key is None or "".join(key[0].split()) != "".join(public.split()):
        raise ValueError("toolbox signing key differs from the key embedded in the runner")
    subprocess.run([
        "cosign", "verify-blob", "--insecure-ignore-tlog", "--key",
        str(root / "toolbox" / "release.pub"), "--signature",
        str(root / "toolbox" / f"toolbox-{digest}.json.sig"),
        str(root / "toolbox" / f"toolbox-{digest}.json"),
    ], check=True)
