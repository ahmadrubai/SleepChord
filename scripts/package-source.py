"""Prepare a credential-free Windows source candidate and record build provenance."""

import hashlib
import json
from pathlib import Path
import re
import subprocess
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile


ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-pc-windows-msvc"
REDACTIONS = {
    "src/lastfm.rs": (
        r'(?m)^(const (?:API_KEY|SECRET): &str\s*=\s*)"[^"\r\n]*";',
        r'\1""; // Omitted inherited provider credential.',
        2,
    ),
    "src/transfer/spotify.rs": (
        r"(?ms)^const FALLBACK_SECRET:[^=]+=.*?^\);",
        'const FALLBACK_SECRET: (u32, &[u8]) = (0, b""); // Omitted inherited key material.',
        1,
    ),
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def cargo_graph():
    output = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--offline", "--filter-platform", TARGET, "--format-version", "1"],
        cwd=ROOT,
    )
    return json.loads(output)


def locked_checksums():
    result = {}
    for block in (ROOT / "Cargo.lock").read_text(encoding="utf-8").split("[[package]]")[1:]:
        fields = dict(re.findall(r'^([a-z]+) = "([^"]+)"$', block, re.MULTILINE))
        if fields.get("checksum"):
            result[(fields["name"], fields["version"], fields["source"])] = fields["checksum"]
    return result


def source_files():
    files = [ROOT / name for name in ("Cargo.toml", "Cargo.lock", "build.rs", "LICENSE", "README.md", "CODE_SIGNING.md")]
    for directory, suffixes in {
        "src": {".rs"},
        "assets": {".ico", ".png", ".svg", ".ttf", ".txt", ".jpg", ".json", ".js"},
        "scripts": {".ps1", ".py"},
        "installer": {".iss", ".txt"},
    }.items():
        files.extend(
            path for path in (ROOT / directory).rglob("*")
            if path.is_file() and path.suffix in suffixes
            and not any(part.startswith(".") or part in {"Output", "__pycache__"} for part in path.relative_to(ROOT).parts)
        )
    files.extend(path for path in (ROOT / "docs").glob("windows-*") if path.is_file())
    files.extend(path for path in (ROOT / "docs" / "licenses").iterdir() if path.suffix in {".txt", ".tsv"})
    return sorted(set(files))


def verify_artifact(folder, filename):
    base = ROOT / "dist" / folder
    expected = dict(
        (name, digest) for digest, name in (
            line.split("  ", 1) for line in (base / "SHA256SUMS.txt").read_text(encoding="ascii").splitlines()
        )
    )
    actual = sha256((base / filename).read_bytes())
    if expected.get(filename) != actual:
        raise ValueError(f"Artifact checksum differs from the release manifest: {filename}")
    return actual


def add_dependencies(archive, graph):
    nodes = {node["id"] for node in graph["resolve"]["nodes"]}
    checksums = locked_checksums()
    manifest = {}
    for dep in sorted(graph["packages"], key=lambda item: (item["name"], item["version"])):
        if dep["id"] not in nodes or dep["source"] is None:
            continue
        name, version, source = dep["name"], dep["version"], dep["source"]
        if not source.startswith("registry+"):
            raise ValueError(f"Dependency requires a different source export method: {name} {version}")
        crate = Path(dep["manifest_path"]).parent
        file = f"{name}-{version}.crate"
        data = (crate.parents[2] / "cache" / crate.parent.name / file).read_bytes()
        digest = sha256(data)
        if checksums.get((name, version, source)) != digest:
            raise ValueError(f"Dependency archive differs from Cargo.lock: {file}")
        entry = f"dependency-sources/{file}"
        archive.writestr(entry, data, compress_type=ZIP_STORED)
        manifest[entry] = digest
    return manifest


def main():
    graph = cargo_graph()
    app = next(package for package in graph["packages"] if package["id"] == graph["resolve"]["root"])
    version = app["version"]
    installer = f"SleepChord-Setup-{version}-x64.exe"
    metadata = {
        "status": "Local source candidate; credential redactions prevent exact reproduction.",
        "version": version,
        "target": TARGET,
        "rustc": subprocess.check_output(["rustc", "-vV"], cwd=ROOT, text=True).strip(),
        "binary_sha256": {
            "SleepChord.exe": verify_artifact("windows-x64", "SleepChord.exe"),
            installer: verify_artifact("installer", installer),
        },
        "source_sha256": {},
        "redacted": sorted(REDACTIONS),
    }
    output = ROOT / "dist" / "source"
    output.mkdir(exist_ok=True)
    package = output / f"SleepChord-{version}-windows-x64-source-candidate.zip"
    staging = package.with_suffix(".tmp")
    try:
        with ZipFile(staging, "w", ZIP_DEFLATED, compresslevel=6) as archive:
            for path in source_files():
                relative = path.relative_to(ROOT).as_posix()
                data = path.read_bytes()
                metadata["source_sha256"][relative] = sha256(data)
                if relative in REDACTIONS:
                    pattern, replacement, expected = REDACTIONS[relative]
                    sanitized, count = re.subn(pattern, replacement, data.decode("utf-8"))
                    if count != expected:
                        raise ValueError(f"Redaction rules no longer match {relative}; review before archiving.")
                    data = sanitized.encode("utf-8")
                archive.writestr(relative, data)
            metadata["dependency_source_sha256"] = add_dependencies(archive, graph)
            notices = ROOT / "dist" / "windows-x64" / "THIRD-PARTY-NOTICES.txt"
            archive.writestr("THIRD-PARTY-NOTICES.txt", notices.read_bytes())
            archive.writestr("SOURCE-MANIFEST.json", json.dumps(metadata, indent=2, sort_keys=True) + "\n")
        staging.replace(package)
    finally:
        staging.unlink(missing_ok=True)
    digest = sha256(package.read_bytes())
    (output / "SHA256SUMS.txt").write_text(f"{digest}  {package.name}\n", encoding="ascii")
    print(f"{package} ({package.stat().st_size} bytes; {len(metadata['dependency_source_sha256'])} dependency archives)")
    print(f"SHA-256: {digest}")


if __name__ == "__main__":
    main()
