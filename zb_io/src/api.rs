use zb_core::{Error, Formula};

pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new() -> Self {
        Self::with_base_url("https://formulae.brew.sh/api/formula".to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_formula(&self, name: &str) -> Result<Formula, Error> {
        let url = format!("{}/{}.json", self.base_url, name);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::NetworkFailure {
                message: e.to_string(),
            })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::MissingFormula {
                name: name.to_string(),
            });
        }

        if !response.status().is_success() {
            return Err(Error::NetworkFailure {
                message: format!("HTTP {}", response.status()),
            });
        }

        let formula: Formula = response.json().await.map_err(|e| Error::NetworkFailure {
            message: format!("failed to parse formula JSON: {e}"),
        })?;

        Ok(formula)
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetches_formula_from_mock_server() {
        let mock_server = MockServer::start().await;

        let fixture = include_str!("../../zb_core/fixtures/formula_foo.json");

        Mock::given(method("GET"))
            .and(path("/foo.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri());
        let formula = client.get_formula("foo").await.unwrap();

        assert_eq!(formula.name, "foo");
        assert_eq!(formula.versions.stable, "1.2.3");
    }

    #[tokio::test]
    async fn returns_missing_formula_on_404() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/nonexistent.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = ApiClient::with_base_url(mock_server.uri());
        let err = client.get_formula("nonexistent").await.unwrap_err();

        assert!(matches!(
            err,
            Error::MissingFormula { name } if name == "nonexistent"
        ));
    }
}
