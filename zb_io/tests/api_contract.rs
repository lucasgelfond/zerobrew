//! Smoke tests that verify the Homebrew API still returns the expected schema.
//! These hit the real network and should run on a schedule (nightly CI), not on every PR.
//!
//! If these tests fail, the Homebrew API has changed and zerobrew needs updating.

use std::time::Duration;

/// Raw HTTP contract test — validates API schema independent of our client code.
#[tokio::test]
#[ignore = "network: Homebrew API contract test"]
async fn homebrew_api_returns_expected_formula_schema() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let resp = client
        .get("https://formulae.brew.sh/api/formula/jq.json")
        .send()
        .await
        .expect("failed to reach Homebrew API (timeout 30s)");

    assert!(resp.status().is_success(), "API returned {}", resp.status());

    let json: serde_json::Value = resp.json().await.expect("invalid JSON");

    // Assert required fields exist with expected types
    assert!(json["name"].is_string(), "missing 'name' field");
    assert!(
        json["versions"]["stable"].is_string(),
        "missing 'versions.stable'"
    );
    assert!(
        json["bottle"]["stable"]["files"].is_object(),
        "missing bottle files"
    );

    // Assert at least one platform bottle exists
    let files = json["bottle"]["stable"]["files"].as_object().unwrap();
    assert!(!files.is_empty(), "no bottle files");

    // Assert bottle entry has required fields
    let (_, first_bottle) = files.iter().next().unwrap();
    assert!(first_bottle["url"].is_string(), "bottle missing 'url'");
    assert!(
        first_bottle["sha256"].is_string(),
        "bottle missing 'sha256'"
    );
}

/// Smoke test through our ApiClient — validates client-level parsing and caching.
/// 60s timeout to prevent CI hangs.
#[tokio::test(flavor = "current_thread")]
#[ignore = "network: Homebrew API contract test via ApiClient"]
async fn api_client_parses_formula_correctly() {
    use zb_io::ApiClient;
    let client = ApiClient::new();
    let formula = tokio::time::timeout(Duration::from_secs(60), client.get_formula("jq"))
        .await
        .expect("ApiClient timed out after 60s")
        .expect("ApiClient failed to fetch jq");
    assert_eq!(formula.name, "jq");
    assert!(
        !formula.versions.stable.is_empty(),
        "missing stable version"
    );
}

#[tokio::test]
#[ignore = "network: Homebrew API contract test"]
async fn homebrew_api_404_for_nonexistent_formula() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let resp = client
        .get("https://formulae.brew.sh/api/formula/this-formula-does-not-exist-zerobrew-test.json")
        .send()
        .await
        .expect("failed to reach Homebrew API");

    assert_eq!(resp.status().as_u16(), 404);
}
