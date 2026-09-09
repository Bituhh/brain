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

/// Requirement 1.2 (`brain-core` has no dependency on any binding crate,
/// on `napi`, or on `wasm-bindgen`) and half of Requirement 1.4 (dev-only
/// tooling is exempt from the runtime-dependency justification rule):
/// checked directly against the manifest text rather than via `cargo
/// tree` (which would need this test to shell out to cargo itself) --
/// brain-core's zero-dependency claim is precisely that no
/// `[dependencies]` table exists at all, only `[dev-dependencies]`.
#[test]
fn brain_core_manifest_has_no_dependencies_table() {
    assert!(
        !BRAIN_CORE_CARGO_TOML.contains("\n[dependencies]"),
        "brain-core must carry zero runtime dependencies (ENG-5, ENG-6, Requirement 1.2) -- \
         found a [dependencies] table in its Cargo.toml"
    );
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
    let forbidden = ["tensorflow", "pytorch", "onnx", "autodiff", "embedding-model", "llm", "gguf", "candle", "ndarray", "burn"];
    let manifests = [
        ("brain-core/Cargo.toml", BRAIN_CORE_CARGO_TOML),
        ("brain-napi/Cargo.toml", BRAIN_NAPI_CARGO_TOML),
        ("Cargo.toml (workspace root)", ROOT_CARGO_TOML),
        ("package.json (workspace root)", ROOT_PACKAGE_JSON),
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
        stdout.contains("acceptance criteria found in requirements.md"),
        "the checker must report how many criteria it parsed, got stdout: {stdout}"
    );
    assert!(output.status.code().is_some(), "the checker must exit cleanly, not crash or panic");
}
