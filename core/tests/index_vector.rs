//! The backend signs the runtime index with its own tooling (Python); this checks that the client verifies that
//! tooling's output, so the two sides cannot drift apart unnoticed. The vector's private key was discarded.

use base64::Engine as _;
use skradle_core::index::{parse_rfc3339, verify, IndexError, TrustedKey};

const VECTOR: &str = include_str!("fixtures/backend_runtimes_vector.json");

fn vector() -> serde_json::Value {
    serde_json::from_str(VECTOR).expect("the fixture is JSON")
}

fn trusted(v: &serde_json::Value) -> Vec<TrustedKey> {
    let public = base64::engine::general_purpose::STANDARD
        .decode(v["publicKey"].as_str().unwrap())
        .expect("public key is base64");
    // TrustedKey ids are 'static in the program; the test leaks one string.
    let key_id: &'static str = Box::leak(v["keyId"].as_str().unwrap().to_owned().into_boxed_str());
    vec![TrustedKey {
        key_id,
        public_key: public.try_into().expect("32-byte key"),
    }]
}

#[test]
fn the_backends_valid_envelope_verifies() {
    let v = vector();
    let now = parse_rfc3339(v["validAt"].as_str().unwrap()).unwrap();
    let index = verify(&v["valid"].to_string(), &trusted(&v), now)
        .expect("a valid envelope from the backend tooling");
    println!(
        "runtimes {}, modules {}, skipped {:?}",
        index.runtimes.len(),
        index.modules.len(),
        index.skipped
    );
    assert!(
        index.skipped.is_empty(),
        "the backend's own sample entries all pass the client's checks: {:?}",
        index.skipped
    );
    assert!(!index.runtimes.is_empty() || !index.modules.is_empty());
}

#[test]
fn the_backends_tampered_envelope_is_rejected() {
    let v = vector();
    let now = parse_rfc3339(v["validAt"].as_str().unwrap()).unwrap();
    let tampered = &v["tamperedPayload"]["envelope"];
    assert!(matches!(
        verify(&tampered.to_string(), &trusted(&v), now),
        Err(IndexError::BadSignature)
    ));
}

#[test]
fn the_backends_envelope_is_refused_after_it_expires() {
    let v = vector();
    let later = parse_rfc3339(v["expiredAt"].as_str().unwrap()).unwrap();
    assert!(matches!(
        verify(&v["valid"].to_string(), &trusted(&v), later),
        Err(IndexError::Expired)
    ));
}

#[test]
fn another_key_does_not_verify_it() {
    let v = vector();
    let now = parse_rfc3339(v["validAt"].as_str().unwrap()).unwrap();
    let mut keys = trusted(&v);
    keys[0].public_key[0] ^= 1;
    assert!(verify(&v["valid"].to_string(), &keys, now).is_err());
}
