//! Cross-implementation differential against the Python verifier.
//!
//! A second implementation is only worth having if it is genuinely independent *and*
//! genuinely agrees. Two checkers that disagree mean one of them is wrong and nobody
//! knows which, so the disagreement has to fail the build rather than be noticed
//! later.
//!
//! The vectors live in `tests/vectors.json`, shared with patchproof's own test suite,
//! so both implementations are checked against the same inputs rather than each
//! against its own convenient ones. Every vector records the status both must return.
//!
//! Skipped when the vector file is absent, so the crate still builds standalone.

use std::path::Path;

use patchproof_verify::{replay_bound, Status};

fn status_from_str(s: &str) -> Status {
    match s {
        "VERIFIED" => Status::Verified,
        "UNVERIFIED" => Status::Unverified,
        "REJECTED" => Status::Rejected,
        other => panic!("unknown expected status {:?} in the vector file", other),
    }
}

#[test]
fn agrees_with_the_python_verifier_on_every_shared_vector() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors.json");
    if !path.exists() {
        eprintln!("skipping: {} not present", path.display());
        return;
    }
    let raw = std::fs::read_to_string(&path).expect("vectors.json is unreadable");
    let vectors: serde_json::Value = serde_json::from_str(&raw).expect("vectors.json is not JSON");
    let cases = vectors["cases"]
        .as_array()
        .expect("vectors.json has no 'cases'");
    assert!(!cases.is_empty(), "vector file is empty");

    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let expected = status_from_str(case["expected"].as_str().expect("no 'expected'"));
        let (got, msg) = replay_bound(&case["certificate"]);
        if got != expected {
            failures.push(format!(
                "  {}: python says {}, rust says {} ({})",
                name, expected, got, msg
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "the two verifiers disagree, so one of them is wrong:\n{}",
        failures.join("\n")
    );
    eprintln!("{} shared vectors, both verifiers agree", cases.len());
}

#[test]
fn no_vector_is_verified_without_a_claim() {
    // A cheap invariant over the corpus itself: if any vector expects VERIFIED while
    // carrying no claim, either the vectors or the binding rule is wrong.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors.json");
    if !path.exists() {
        return;
    }
    let raw = std::fs::read_to_string(&path).unwrap();
    let vectors: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        if case["expected"] == "VERIFIED" {
            assert!(
                case["certificate"]["claim"]["defect_class"].is_string(),
                "vector {:?} expects VERIFIED but binds no defect class",
                case["name"]
            );
        }
    }
}
