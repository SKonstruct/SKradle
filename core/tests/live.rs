//! Checks against the real game servers. Ignored by default (they need the network); run with
//! `cargo test -p skradle-core --test live -- --ignored --nocapture`.

use skradle_core::getdown_txt::GetdownConfig;
use skradle_core::install::{updater_jar_url, LATEST_GETDOWN_TXT_URL};
use skradle_core::net::Client;

#[test]
#[ignore = "needs the network"]
fn the_live_config_is_fetched_over_https_and_plans_an_install() {
    let text = Client::new()
        .get_text(LATEST_GETDOWN_TXT_URL, 1 << 20)
        .expect("fetch getdown.txt over HTTPS");
    let config = GetdownConfig::parse(&text);
    println!(
        "version {:?}, java {:?}",
        config.version,
        config.java_major()
    );
    assert!(
        config.version.as_deref().is_some_and(|v| v.len() >= 12),
        "a numeric version"
    );
    assert!(config.java_major().is_some_and(|major| major >= 17));
    let jar = updater_jar_url(&config).expect("the live config is trusted");
    println!("updater jar: {jar}");
    let bytes = Client::new()
        .get_bytes(&jar, 5 << 20)
        .expect("fetch the updater jar over HTTPS");
    assert!(bytes.starts_with(b"PK"), "a zip/jar file");
}

#[test]
#[ignore = "needs a Sentry DSN in SKRADLE_SENTRY_TEST_DSN"]
fn sends_a_test_event_to_sentry() {
    use skradle_core::telemetry::{Dsn, Level, Report, Reporter};
    use std::collections::BTreeMap;

    let text = std::env::var("SKRADLE_SENTRY_TEST_DSN")
        .expect("set SKRADLE_SENTRY_TEST_DSN to a Sentry DSN");
    let dsn = Dsn::parse(&text).expect("the DSN must look like https://<key>@<host>/<project id>");
    let reporter = Reporter::new(Some(dsn), true, "skradle@test");
    let mut tags = BTreeMap::new();
    tags.insert("kind".to_owned(), "test".to_owned());
    let report = Report {
        level: Level::Error,
        message: "SKradle test event: safe to delete".to_owned(),
        backtrace: None,
        tags,
    };
    reporter
        .send_blocking(&report)
        .expect("Sentry accepted the event");
    println!("sent: check the project's Issues page for 'SKradle test event'");
}
