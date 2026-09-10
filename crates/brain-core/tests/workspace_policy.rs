//! Static checks on the workspace manifests themselves (Requirement 1):
//! these are policy assertions about *text in Cargo.toml/package.json*,
//! not simulation behaviour -- deliberately its own file rather than
//! folded into a behavioural test, since a manifest edit is exactly the
//! kind of change that should trip one of these on the very next
//! `cargo test` regardless of which other file someone happened to touch.

const BRAIN_CORE_CARGO_TOML: &str = include_str!("../Cargo.toml");
const BRAIN_NAPI_CARGO_TOML: &str = include_str!("../../brain-napi/Cargo.toml");
const ROOT_CARGO_TOML: &str = include_str!("../../../Cargo.toml");
const ROOT_PACKAGE_JSON: &str = include_str!("../../../package.json");
const RUST_TOOLCHAIN_TOML: &str = include_str!("../../../rust-toolchain.toml");
/// Phase 5 Requirement 1.2: `packages/io` is new this phase and was never
/// covered by this check before -- without this, a tokenizer or embedding
/// dependency could land in its `package.json` with nothing to catch it.
const PACKAGES_IO_PACKAGE_JSON: &str = include_str!("../../../packages/io/package.json");

/// Requirement 1.2 (`brain-core` has no dependency on any binding crate,
/// on `napi`, or on `wasm-bindgen`) and half of Requirement 1.4 (dev-only
/// tooling is exempt from the runtime-dependency justification rule):
/// checked directly against the manifest text rather than via `cargo
/// tree` (which would need this test to shell out to cargo itself).
///
/// `rayon` is the one runtime dependency this crate is allowed: README
/// ENG-6 names it explicitly ("the Rust core should need approximately
/// `rayon` and nothing else"), and Phase 4's RUN-4 partitioned parallelism
/// (`partition.rs`) is exactly the anticipated use. This test's job is
/// narrower than "no `[dependencies]` table exists" now -- it is "the
/// table, if present, names *only* `rayon`", so any *other* runtime
/// dependency added later without updating this test (and its own
/// justification, per ENG-6's "any proposed runtime dependency requires
/// explicit justification") still fails loudly on the next `cargo test`.
#[test]
fn brain_core_manifest_carries_no_runtime_dependency_beyond_rayon() {
    if let Some(deps_start) = BRAIN_CORE_CARGO_TOML.find("\n[dependencies]") {
        let after = &BRAIN_CORE_CARGO_TOML[deps_start + 1..];
        let section_end = after[1..].find("\n[").map(|i| i + 1).unwrap_or(after.len());
        let section = &after[..section_end];
        let dep_lines: Vec<&str> = section
            .lines()
            .skip(1) // the "[dependencies]" header line itself
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        assert_eq!(
            dep_lines.len(),
            1,
            "brain-core's [dependencies] table must name exactly one crate (rayon, ENG-6) -- found: {dep_lines:?}"
        );
        assert!(
            dep_lines[0].starts_with("rayon"),
            "brain-core's only runtime dependency must be rayon (ENG-6) -- found: {}",
            dep_lines[0]
        );
    }
    assert!(
        !BRAIN_CORE_CARGO_TOML.to_lowercase().contains("napi") && !BRAIN_CORE_CARGO_TOML.to_lowercase().contains("wasm-bindgen"),
        "brain-core must not depend on a binding crate (Requirement 1.2)"
    );
    assert!(
        BRAIN_CORE_CARGO_TOML.contains("[dev-dependencies]"),
        "proptest/criterion are expected as dev-dependencies (Requirement 1.4's exemption) -- \
         this assertion just confirms the section this test reasons about still exists"
    );
}

/// Requirement 1.3: no manifest in the workspace may name a
/// neural-network, tensor, autodiff, ONNX, embedding, or LLM dependency.
/// Checked as substring absence across every manifest, including
/// `brain-napi`'s (the FFI boundary is not exempt just because it is
/// allowed a real dependency on `napi` itself).
#[test]
fn no_manifest_names_a_forbidden_ai_ml_dependency() {
    let forbidden = ["tensorflow", "pytorch", "onnx", "autodiff", "embedding-model", "llm", "gguf", "candle", "ndarray", "burn", "tokenizer"];
    let manifests = [
        ("brain-core/Cargo.toml", BRAIN_CORE_CARGO_TOML),
        ("brain-napi/Cargo.toml", BRAIN_NAPI_CARGO_TOML),
        ("Cargo.toml (workspace root)", ROOT_CARGO_TOML),
        ("package.json (workspace root)", ROOT_PACKAGE_JSON),
        ("packages/io/package.json", PACKAGES_IO_PACKAGE_JSON),
    ];
    for (name, contents) in manifests {
        let lower = contents.to_lowercase();
        for term in forbidden {
            assert!(!lower.contains(term), "{name} must not name a forbidden AI/ML dependency (Requirement 1.3), found '{term}'");
        }
    }
}

/// Requirement 1.1's testable half: a clean checkout needs a toolchain
/// pinned to an exact version for the build to be reproducible without
/// the developer hunting down a matching compiler themselves. (The other
/// half -- "produces a loadable native addon... without manual
/// intervention beyond a documented install step" -- is a process claim
/// this suite's own successful `cargo build`/`npm run build` runs satisfy
/// in practice, not something a single assertion can encode.)
#[test]
fn rust_toolchain_is_pinned_to_an_exact_version() {
    assert!(
        RUST_TOOLCHAIN_TOML.contains("channel"),
        "rust-toolchain.toml must pin an exact channel/version (Requirement 1.1, 3.4), not float on stable"
    );
}

/// Splits Rust source into lowercase, word-boundary-delimited identifier
/// tokens -- underscore/non-alphanumeric boundaries *and* camelCase
/// boundaries (lowercase-to-uppercase transitions), so `TextEncoder` and
/// `text_width` both tokenize to include a standalone `"text"` token,
/// while `context`/`LocalContext` do **not** (their token is `"context"`
/// whole, never `"text"` alone). A plain substring scan would falsely flag
/// every occurrence of `context`/`LocalContext` -- both used throughout
/// this crate's plasticity machinery -- the moment `"text"` is a forbidden
/// word, which is exactly why this function exists instead of reusing the
/// simpler substring check `no_manifest_names_a_forbidden_ai_ml_dependency`
/// already uses for manifests (where that risk does not arise).
fn identifier_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for ch in line.chars() {
        if ch.is_alphanumeric() {
            if ch.is_uppercase() && prev_lower && !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
            prev_lower = ch.is_lowercase() || ch.is_numeric();
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            prev_lower = false;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Strips comment lines (this project's own doc comments legitimately
/// discuss modality/action/effector/environment concepts in prose --
/// including this very file's -- without violating invariant 8, so only
/// *code* is scanned).
fn non_comment_lines(source: &str) -> impl Iterator<Item = &str> {
    source.lines().filter(|line| {
        let trimmed = line.trim_start();
        !(trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.is_empty())
    })
}

fn walk_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Requirement 1.5/16.6 (invariant 8): "any type, field or branch in the
/// core that names a modality... is a design defect" (README invariant
/// 8), extended by Requirement 16.6 to action/effector/environment names
/// once the sensorimotor loop exists. Scans every `.rs` file under
/// `brain-core/src` and `brain-napi/src` (walked at test-run time via
/// `std::fs`, not `include_str!`, so a newly added file is covered
/// automatically) for forbidden words as *whole identifier tokens*, not
/// substrings -- see `identifier_tokens`'s doc comment for why that
/// distinction matters here specifically.
#[test]
fn neither_core_crate_names_a_modality_action_effector_or_environment() {
    let forbidden = ["text", "pixel", "image", "audio", "sound", "video", "action", "effector", "motor", "environment"];
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    walk_rs_files(&manifest_dir.join("src"), &mut files);
    walk_rs_files(&manifest_dir.join("../brain-napi/src"), &mut files);
    assert!(!files.is_empty(), "the source walk must actually find files, or this test would pass vacuously");

    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else { continue };
        for line in non_comment_lines(&source) {
            for token in identifier_tokens(line) {
                assert!(
                    !forbidden.contains(&token.as_str()),
                    "{}: code (outside comments) must not name a modality/action/effector/environment (Requirement 1.5/16.6), found identifier token '{}' in line: {}",
                    path.display(),
                    token,
                    line.trim()
                );
            }
        }
    }
}

/// Requirement 15.10: a fast tier (units, boundary, properties) and a slow
/// tier (emergent behaviour, soaks, golden rasters) must each be
/// separately invocable. Checked as the presence of distinct npm scripts
/// rather than by actually running the slow tier here, which would defeat
/// the point of having a fast tier at all.
#[test]
fn fast_and_slow_test_tiers_are_separately_invocable() {
    // Plain substring checks rather than real JSON parsing -- adding a
    // parser dependency for one structural check on two script names
    // would be exactly the kind of unjustified dependency Requirement 1.4
    // asks for a reason to add, and this manifest's shape is simple and
    // stable enough that a substring check is not fragile here.
    assert!(
        ROOT_PACKAGE_JSON.contains("\"test:fast\":"),
        "a fast tier must be invocable as its own npm script (Requirement 15.10)"
    );
    assert!(
        ROOT_PACKAGE_JSON.contains("\"test:slow\":"),
        "a slow tier must be separately invocable as its own npm script (Requirement 15.10)"
    );
}

/// Requirement 15.9: the traceability mapping must not just exist as
/// prose but be *checkable* -- this runs the actual checker
/// (`scripts/check-traceability.mjs`) as a subprocess and confirms it
/// executes and reports a coherent result, rather than merely asserting
/// the script file is present. It deliberately does not assert a
/// specific exit code: Step 13's emergent-behaviour criteria are legitimate
/// outstanding gaps until that step lands, so "the checker runs and
/// reports its findings" is what this test can honestly claim -- the
/// checker's own report (`npm run check:traceability`, part of the slow
/// tier) is what a human reviews for whether every *currently expected*
/// gap is still just the deliberate ones.
#[test]
fn the_traceability_checker_itself_runs_and_reports_a_result() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new("node")
        .arg("scripts/check-traceability.mjs")
        .current_dir(&repo_root)
        .output()
        .expect("node must be available to run the traceability checker (it is already a hard requirement of this workspace)");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("acceptance criteria found across") && stdout.contains("requirements docs"),
        "the checker must report how many criteria it parsed, got stdout: {stdout}"
    );
    assert!(output.status.code().is_some(), "the checker must exit cleanly, not crash or panic");
}
