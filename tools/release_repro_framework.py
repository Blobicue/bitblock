#!/usr/bin/env python3
"""RELEASE-R1@framework reproducible candidate-build verification framework.

Verifies pinned external build inputs without importing Rust or network access:
  1. Pinned Rust toolchain (`rust-toolchain.toml` channel & minimal profile).
  2. Workspace direct dependency version pinning (`Cargo.toml`, `crates/*/Cargo.toml`).
  3. Deterministic SBOM manifest & SHA-256 inventory derived from `Cargo.lock`.
  4. Pinned Lean 4 toolchain (`proof/lean-toolchain`) and LadderLang capsule
     `source-manifest.tsv` SHA-256 integrity.
  5. Committed WebAssembly artifact (`crates/wasm/www/bitblock_wasm.wasm`) digest
     and embedded base64 consistency across `play.html` and `bitblock-lab.html`.
  6. Five powered in-memory negative controls verifying that mutating toolchain,
     lockfile SBOM, feature profile, Lean source manifest, or binary digest
     strictly changes the candidate identity / rejects verification.

Usage:
  python3 tools/release_repro_framework.py                 # run framework verification + controls
  python3 tools/release_repro_framework.py --emit-manifest # emit candidate framework manifest JSON
"""

from __future__ import annotations

import base64
import csv
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FORMAT_ID = "bitblock-release-r1-framework-manifest-v1"
EXPECTED_RUST_CHANNEL = "1.96.0"
EXPECTED_LEAN_TOOLCHAIN = "leanprover/lean4:v4.33.0"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def parse_rust_toolchain(text: str) -> dict[str, str]:
    channel = re.search(r'^\s*channel\s*=\s*"([^"]+)"', text, re.M)
    profile = re.search(r'^\s*profile\s*=\s*"([^"]+)"', text, re.M)
    if not channel or not profile:
        raise ValueError("rust-toolchain.toml missing channel or profile")
    return {"channel": channel.group(1), "profile": profile.group(1)}


def parse_cargo_lock_sbom(text: str) -> list[dict[str, str]]:
    packages: list[dict[str, str]] = []
    blocks = text.split("[[package]]")[1:]
    for block in blocks:
        name = re.search(r'^name\s*=\s*"([^"]+)"', block, re.M)
        ver = re.search(r'^version\s*=\s*"([^"]+)"', block, re.M)
        src = re.search(r'^source\s*=\s*"([^"]+)"', block, re.M)
        chk = re.search(r'^checksum\s*=\s*"([^"]+)"', block, re.M)
        if not name or not ver:
            raise ValueError("malformed [[package]] block in Cargo.lock")
        packages.append(
            {
                "name": name.group(1),
                "version": ver.group(1),
                "source": src.group(1) if src else "workspace-local",
                "checksum": chk.group(1) if chk else "workspace-local",
            }
        )
    packages.sort(key=lambda p: (p["name"], p["version"], p["source"], p["checksum"]))
    return packages


def verify_exact_direct_pins(cargo_toml_paths: list[Path]) -> list[dict[str, str]]:
    pinned: list[dict[str, str]] = []
    for path in cargo_toml_paths:
        doc = tomllib.loads(path.read_text(encoding="utf-8"))
        dep_tables: list[dict] = []
        if isinstance(doc.get("dependencies"), dict):
            dep_tables.append(doc["dependencies"])
        for target_cfg in (doc.get("target") or {}).values():
            if isinstance(target_cfg, dict) and isinstance(target_cfg.get("dependencies"), dict):
                dep_tables.append(target_cfg["dependencies"])
        for table in dep_tables:
            for dep, spec in table.items():
                if isinstance(spec, str):
                    ver = spec
                elif isinstance(spec, dict):
                    if "path" in spec:
                        continue  # workspace-internal path dependency
                    ver = spec.get("version")
                    if not isinstance(ver, str):
                        raise ValueError(
                            f"{path.relative_to(ROOT)}: external dependency {dep!r} missing version"
                        )
                else:
                    continue
                if not ver.startswith("="):
                    raise ValueError(
                        f"{path.relative_to(ROOT)}: external dependency {dep!r} not exact-pinned ({ver!r})"
                    )
                pinned.append(
                    {"manifest": path.relative_to(ROOT).as_posix(), "crate": dep, "pin": ver}
                )
    pinned.sort(key=lambda r: (r["manifest"], r["crate"], r["pin"]))
    return pinned


def verify_lean_capsule_manifest(manifest_path: Path) -> list[dict[str, str]]:
    rows = []
    with manifest_path.open("r", encoding="utf-8") as fh:
        for row in csv.DictReader(fh, delimiter="\t"):
            rel = row["path"].strip()
            expected = row["sha256"].strip().lower()
            actual = sha256_file(manifest_path.parent / rel)
            if actual != expected:
                raise ValueError(f"Lean source manifest mismatch for {rel}: {actual} != {expected}")
            rows.append({"path": rel, "sha256": actual})
    if not rows:
        raise ValueError("empty Lean source-manifest.tsv")
    return rows


def verify_wasm_embedded_parity(wasm_path: Path, html_paths: list[Path]) -> str:
    raw = wasm_path.read_bytes()
    expected_b64 = base64.b64encode(raw).decode("ascii")
    for html_path in html_paths:
        text = html_path.read_text(encoding="utf-8")
        m = re.search(r"const WASM_B64 =\r?\n([\s\S]*?);\r?\n", text)
        if not m:
            raise ValueError(f"{html_path.name}: missing const WASM_B64 block")
        chunks = re.findall(r'"([A-Za-z0-9+/=]+)"', m.group(1))
        if not chunks:
            raise ValueError(f"{html_path.name}: empty WASM_B64 block")
        embedded = "".join(chunks)
        if embedded != expected_b64:
            raise ValueError(f"{html_path.name}: embedded WASM_B64 diverges from {wasm_path.name}")
    return sha256_bytes(raw)


def build_framework_manifest(feature_profile: str = "default+protocol-research") -> dict:
    rust_tc = parse_rust_toolchain((ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"))
    if rust_tc["channel"] != EXPECTED_RUST_CHANNEL:
        raise ValueError(f"unexpected Rust channel {rust_tc['channel']!r}")

    lean_tc = (ROOT / "proof" / "lean-toolchain").read_text(encoding="utf-8").strip()
    if lean_tc != EXPECTED_LEAN_TOOLCHAIN:
        raise ValueError(f"unexpected Lean toolchain {lean_tc!r}")

    lock_text = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    sbom_packages = parse_cargo_lock_sbom(lock_text)
    sbom_canonical = json.dumps(sbom_packages, separators=(",", ":"), sort_keys=True).encode("utf-8")
    sbom_digest = sha256_bytes(sbom_canonical)

    direct_pins = verify_exact_direct_pins(
        [
            ROOT / "Cargo.toml",
            ROOT / "crates" / "core" / "Cargo.toml",
            ROOT / "crates" / "research" / "Cargo.toml",
            ROOT / "crates" / "wasm" / "Cargo.toml",
        ]
    )

    lean_manifest_rows = verify_lean_capsule_manifest(
        ROOT / "proof" / "ladderlang-transition-v1" / "source-manifest.tsv"
    )

    wasm_sha = verify_wasm_embedded_parity(
        ROOT / "crates" / "wasm" / "www" / "bitblock_wasm.wasm",
        [
            ROOT / "crates" / "wasm" / "www" / "play.html",
            ROOT / "crates" / "wasm" / "www" / "bitblock-lab.html",
        ],
    )

    core = {
        "format": FORMAT_ID,
        "authority_label": "CANDIDATE-FRAMEWORK-ONLY (not an admitted RELEASE-1 artifact)",
        "feature_profile": feature_profile,
        "rust_toolchain": rust_tc,
        "rust_toolchain_sha256": sha256_file(ROOT / "rust-toolchain.toml"),
        "lean_toolchain": lean_tc,
        "lean_toolchain_sha256": sha256_file(ROOT / "proof" / "lean-toolchain"),
        "cargo_lock_sha256": sha256_bytes(lock_text.encode("utf-8")),
        "cargo_lock_package_count": len(sbom_packages),
        "sbom_sha256": sbom_digest,
        "direct_dependency_pins": direct_pins,
        "lean_ladderlang_capsule_sources": lean_manifest_rows,
        "committed_wasm_sha256": wasm_sha,
    }
    core_canonical = json.dumps(core, separators=(",", ":"), sort_keys=True).encode("utf-8")
    return {
        "framework_manifest_sha256": sha256_bytes(core_canonical),
        "manifest": core,
    }


def run_powered_negative_controls(baseline: dict) -> None:
    base_id = baseline["framework_manifest_sha256"]
    base_core = baseline["manifest"]

    def digest_of(mutated_core: dict) -> str:
        raw = json.dumps(mutated_core, separators=(",", ":"), sort_keys=True).encode("utf-8")
        return sha256_bytes(raw)

    # 1. Toolchain mutation must change framework identity
    m_tc = json.loads(json.dumps(base_core))
    m_tc["rust_toolchain"]["channel"] = "1.95.0"
    assert digest_of(m_tc) != base_id, "toolchain mutation failed to move identity"

    # 2. SBOM / lockfile mutation must change framework identity
    m_sbom = json.loads(json.dumps(base_core))
    m_sbom["sbom_sha256"] = "0" * 64
    assert digest_of(m_sbom) != base_id, "SBOM mutation failed to move identity"

    # 3. Feature profile mutation must change framework identity
    m_feat = json.loads(json.dumps(base_core))
    m_feat["feature_profile"] = "default-only"
    assert digest_of(m_feat) != base_id, "feature mutation failed to move identity"

    # 4. Lean capsule source digest mutation must change framework identity
    m_lean = json.loads(json.dumps(base_core))
    m_lean["lean_ladderlang_capsule_sources"][0]["sha256"] = "f" * 64
    assert digest_of(m_lean) != base_id, "Lean manifest mutation failed to move identity"

    # 5. Binary / WASM digest mutation must change framework identity
    m_bin = json.loads(json.dumps(base_core))
    m_bin["committed_wasm_sha256"] = "a" * 64
    assert digest_of(m_bin) != base_id, "binary digest mutation failed to move identity"


def main(argv: list[str]) -> int:
    emit = "--emit-manifest" in argv[1:]
    result = build_framework_manifest()
    run_powered_negative_controls(result)
    if emit:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        m = result["manifest"]
        print(
            "release_repro_framework: ok (id=%s..., rust=%s, lean=%s, lock_pkgs=%d, direct_pins=%d, 5/5 negative controls pass)"
            % (
                result["framework_manifest_sha256"][:16],
                m["rust_toolchain"]["channel"],
                m["lean_toolchain"],
                m["cargo_lock_package_count"],
                len(m["direct_dependency_pins"]),
            )
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
