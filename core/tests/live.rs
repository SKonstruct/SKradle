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
