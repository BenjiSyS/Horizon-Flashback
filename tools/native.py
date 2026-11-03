#!/usr/bin/env python3
"""Stage and check the native bundle without modifying upstream checkouts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
PINS = json.loads((ROOT / "compatibility.json").read_text())


def run(args, cwd=ROOT):
    subprocess.run([str(arg) for arg in args], cwd=cwd, check=True)


def revision(path):
    return subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()


def bundle():
    # Copy only explicit source inputs: never the ignored reference trees or recordings.
    inputs = {}
    for path in (ROOT / "native").iterdir():
        if path.suffix in (".rs", ".toml"):
            inputs[path.name] = path.read_bytes()
    inputs["engine/Cargo.toml"] = (ROOT / "Cargo.toml").read_bytes()
    for path in (ROOT / "src").glob("*.rs"):
        inputs[f"engine/src/{path.name}"] = path.read_bytes()
    signature = hashlib.sha256()
    for name, data in sorted(inputs.items()):
        signature.update(name.encode() + b"\0" + len(data).to_bytes(8, "little") + data)
    root = ROOT / "target" / "native-bundles" / signature.hexdigest() / "flashback"
    if root.exists():
        existing = {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob("*") if p.is_file()}
        if existing != inputs:
            raise ValueError(f"Immutable bundle was modified or interrupted: {root}")
    else:
        for name, data in inputs.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    return root.parent


def source(horizon):
    pin = PINS["horizon_revision"]
    subprocess.run(["git", "-C", str(horizon), "cat-file", "-e", pin + "^{commit}"], check=True)
    output = ROOT / "target" / "native-source" / pin
    marker = output / ".flashback-source-pin"
    if marker.exists() and marker.read_text() == pin:
        (output / ".horizon-revision").write_text(pin + "\n")
        return output
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError(f"Incomplete export exists; inspect before retrying: {output}")
    archive = ROOT / "target" / "horizon-source.tar"
    try:
        run(["git", "-C", horizon, "archive", "--format=tar", "--output", archive, pin])
        with tarfile.open(archive) as tar:
            # Reject links and traversal even though this is a pinned trusted repository.
            members = []
            for entry in tar.getmembers():
                resolved = (output / entry.name).resolve()
                if not resolved.is_relative_to(output.resolve()):
                    raise ValueError(f"Unsafe archive member: {entry.name}")
                if entry.issym() or entry.islnk():
                    print(f"Skipping source documentation link: {entry.name}")
                    continue
                members.append(entry)
            tar.extractall(output, members=members, filter="data")
        marker.write_text(pin)
        (output / ".horizon-revision").write_text(pin + "\n")
    except BaseException:
        # No recursive cleanup: leave failed export for inspection.
        raise
    finally:
        archive.unlink(missing_ok=True)
    return output


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("action", choices=["stage", "plan", "apply", "check", "build", "test-net", "verify"])
    parser.add_argument("--horizon", type=Path, default=ROOT / ".upstream" / "horizon")
    parser.add_argument("--mixins", type=Path, default=ROOT / ".upstream" / "mixins")
    args = parser.parse_args()
    mods = bundle()
    if args.action == "stage":
        print(mods)
        return
    if revision(args.mixins) != PINS["horizon_mixins_revision"]:
        raise ValueError("horizon-mixins checkout does not match compatibility.json")
    run(["git", "-C", args.mixins, "diff", "--quiet", "HEAD", "--", "Cargo.toml", "Cargo.lock", "crates"])
    workspace = source(args.horizon)
    run(["cargo", "build", "--locked", "--manifest-path", args.mixins / "Cargo.toml", "-p", "horizon-mixins-cli"])
    cli = args.mixins / "target" / "debug" / ("horizon-mixins.exe" if os.name == "nt" else "horizon-mixins")
    action = "plan" if args.action == "plan" else "apply"
    source_signature = hashlib.sha256()
    for path in sorted(mods.rglob("*")):
        if path.is_file():
            source_signature.update(str(path.relative_to(mods)).encode())
            source_signature.update(path.read_bytes())
    signature = source_signature.hexdigest()[:16]
    # Horizon Mixins cannot atomically replace directories on Windows. Each
    # immutable bundle revision therefore gets a fresh output path.
    output = ROOT / "target" / (f"native-plan-{signature}.json" if action == "plan" else f"native-stages/{signature}")
    run([cli, action, "--workspace", workspace, "--mods", mods, "--out", output])
    if args.action in ("check", "build", "verify"):
        run(["cargo", "check" if args.action == "verify" else args.action, "--locked", "--target-dir", ROOT / "target" / "native-game" / "target", "-p", "horizon-client", "--no-default-features"], cwd=output)
    if args.action in ("test-net", "verify"):
        run(["cargo", "test", "--locked", "--target-dir", ROOT / "target" / "native-game" / "target", "-p", "horizon-client-lib", "--no-default-features", "--lib", "net::flashback_net::tests", "--", "--test-threads=1"], cwd=output)
    if args.action == "verify":
        run(["cargo", "clippy", "--locked", "--target-dir", ROOT / "target" / "native-game" / "target", "-p", "horizon-client-lib", "--no-default-features", "--lib", "--", "-D", "warnings"], cwd=output)
    evidence = {
        "action": args.action, "horizon": PINS["horizon_revision"],
        "mixins": PINS["horizon_mixins_revision"],
        "stage": str(output),
        "bundle_files": {str(p.relative_to(mods)): hashlib.sha256(p.read_bytes()).hexdigest()
                         for p in mods.rglob("*") if p.is_file()},
    }
    (ROOT / "target" / "native-validation.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
