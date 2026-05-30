use std::collections::{BTreeMap, HashSet};

use tracing::warn;
use zb_core::{BuildPlan, Error, Formula, InstallMethod, select_bottle};

use super::{InstallPlan, Installer, PlanFailure, PlannedInstall};

impl Installer {
    pub async fn plan(&self, names: &[String]) -> Result<InstallPlan, Error> {
        self.plan_with_options(names, false).await
    }

    pub async fn plan_with_options(
        &self,
        names: &[String],
        build_from_source: bool,
    ) -> Result<InstallPlan, Error> {
        let formulas = self.fetch_all_formulas(names).await?;
        let ordered = zb_core::resolve_closure(names, &formulas)?;

        let mut items = Vec::with_capacity(ordered.len());
        for install_name in ordered {
            let formula = formulas.get(&install_name).cloned().unwrap();
            let method = if build_from_source {
                match BuildPlan::from_formula(&formula, &self.prefix) {
                    Some(plan) => InstallMethod::Source(plan),
                    None => match select_bottle(&formula) {
                        Ok(bottle) => InstallMethod::Bottle(bottle),
                        Err(_) => {
                            return Err(Error::UnsupportedBottle {
                                name: formula.name.clone(),
                            });
                        }
                    },
                }
            } else {
                match select_bottle(&formula) {
                    Ok(bottle) => InstallMethod::Bottle(bottle),
                    Err(_) => match BuildPlan::from_formula(&formula, &self.prefix) {
                        Some(plan) => InstallMethod::Source(plan),
                        None => {
                            return Err(Error::UnsupportedBottle {
                                name: formula.name.clone(),
                            });
                        }
                    },
                }
            };
            items.push(PlannedInstall {
                install_name,
                formula,
                method,
            });
        }

        Ok(InstallPlan { items })
    }

    pub async fn plan_best_effort(
        &self,
        names: &[String],
        build_from_source: bool,
    ) -> (InstallPlan, Vec<PlanFailure>) {
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut failures = Vec::new();

        for name in names {
            match self
                .plan_with_options(std::slice::from_ref(name), build_from_source)
                .await
            {
                Ok(plan) => {
                    for item in plan.items {
                        if seen.insert(item.install_name.clone()) {
                            items.push(item);
                        }
                    }
                }
                Err(error) => failures.push(PlanFailure {
                    name: name.clone(),
                    error,
                }),
            }
        }

        (InstallPlan { items }, failures)
    }

    async fn fetch_all_formulas(
        &self,
        names: &[String],
    ) -> Result<BTreeMap<String, Formula>, Error> {
        use std::collections::HashSet;

        let mut formulas = BTreeMap::new();
        let mut fetched: HashSet<String> = HashSet::new();
        let mut to_fetch: Vec<String> = names.to_vec();

        while !to_fetch.is_empty() {
            let batch: Vec<String> = to_fetch
                .drain(..)
                .filter(|n| !fetched.contains(n))
                .collect();

            if batch.is_empty() {
                break;
            }

            for n in &batch {
                fetched.insert(n.clone());
            }

            let futures: Vec<_> = batch
                .iter()
                .map(|n| self.api_client.get_formula(n))
                .collect();

            let results = futures::future::join_all(futures).await;

            for (i, result) in results.into_iter().enumerate() {
                let formula = match result {
                    Ok(f) => f,
                    Err(e) => return Err(e),
                };

                if select_bottle(&formula).is_err() && !formula.has_source_url() {
                    warn!(
                        formula = %formula.name,
                        "skipping formula with no bottle or source available for this platform"
                    );
                    continue;
                }

                for dep in formula.runtime_dependencies() {
                    if !fetched.contains(&dep) && !to_fetch.contains(&dep) {
                        to_fetch.push(dep);
                    }
                }

                formulas.insert(batch[i].clone(), formula);
            }
        }

        Ok(formulas)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::cellar::Cellar;
    use crate::installer::install::test_support::*;
    use crate::network::api::ApiClient;
    use crate::storage::blob::BlobCache;
    use crate::storage::db::Database;
    use crate::storage::store::Store;
    use crate::{Installer, Linker};

    #[tokio::test]
    async fn plans_tapped_formula_with_core_dependency() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let dep_bottle = create_bottle_tarball("go");
        let dep_sha = sha256_hex(&dep_bottle);
        let tag = get_test_bottle_tag();
        let dep_json = format!(
            r#"{{
                "name": "go",
                "versions": {{ "stable": "1.24.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/go-1.24.0.{}.bottle.tar.gz",
                                "sha256": "{}"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            tag,
            dep_sha
        );

        Mock::given(method("GET"))
            .and(path("/formula/go.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&dep_json))
            .mount(&mock_server)
            .await;

        let tap_formula_rb = format!(
            r#"
class Terraform < Formula
  version "1.10.0"
  depends_on "go"
  bottle do
    root_url "{}/ghcr/hashicorp/tap"
    sha256 {}: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  end
end
"#,
            mock_server.uri(),
            tag
        );

        Mock::given(method("GET"))
            .and(path("/hashicorp/homebrew-tap/main/Formula/terraform.rb"))
            .respond_with(ResponseTemplate::new(200).set_body_string(tap_formula_rb))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client = ApiClient::with_base_url(format!("{}/formula", mock_server.uri()))
            .unwrap()
            .with_tap_raw_base_url(mock_server.uri());
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.to_path_buf(),
            root.join("locks"),
        );
        let plan = installer
            .plan(&["hashicorp/tap/terraform".to_string()])
            .await
            .unwrap();

        let planned_names: Vec<String> = plan
            .items
            .iter()
            .map(|item| item.formula.name.clone())
            .collect();
        assert!(planned_names.contains(&"terraform".to_string()));
        assert!(planned_names.contains(&"go".to_string()));
    }

    #[tokio::test]
    async fn falls_back_to_source_when_no_bottle() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let formula_json = r#"{
            "name": "nobottle",
            "versions": { "stable": "1.0.0" },
            "dependencies": [],
            "build_dependencies": ["pkgconf"],
            "urls": {
                "stable": {
                    "url": "https://example.com/nobottle-1.0.0.tar.gz",
                    "checksum": "abc123"
                }
            },
            "ruby_source_path": "Formula/n/nobottle.rb",
            "bottle": { "stable": { "files": {} } }
        }"#;

        Mock::given(method("GET"))
            .and(path("/formula/nobottle.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(formula_json))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            root.join("locks"),
        );

        let plan = installer.plan(&["nobottle".to_string()]).await.unwrap();

        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].formula.name, "nobottle");
        assert!(matches!(
            plan.items[0].method,
            zb_core::InstallMethod::Source(_)
        ));

        if let zb_core::InstallMethod::Source(ref bp) = plan.items[0].method {
            assert_eq!(bp.source_url, "https://example.com/nobottle-1.0.0.tar.gz");
            assert_eq!(bp.formula_name, "nobottle");
            assert_eq!(bp.build_dependencies, vec!["pkgconf"]);
        }
    }

    #[tokio::test]
    async fn prefers_bottle_over_source() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "hasboth",
                "versions": {{ "stable": "2.0.0" }},
                "dependencies": [],
                "urls": {{
                    "stable": {{
                        "url": "https://example.com/hasboth-2.0.0.tar.gz",
                        "checksum": "def456"
                    }}
                }},
                "ruby_source_path": "Formula/h/hasboth.rb",
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "https://example.com/hasboth.bottle.tar.gz",
                                "sha256": "aabbccdd"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag
        );

        Mock::given(method("GET"))
            .and(path("/formula/hasboth.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(&formula_json))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            root.join("locks"),
        );

        let plan = installer.plan(&["hasboth".to_string()]).await.unwrap();

        assert_eq!(plan.items.len(), 1);
        assert!(matches!(
            plan.items[0].method,
            zb_core::InstallMethod::Bottle(_)
        ));
    }

    #[tokio::test]
    async fn errors_when_no_bottle_and_no_source() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let formula_json = r#"{
            "name": "nothing",
            "versions": { "stable": "1.0.0" },
            "dependencies": [],
            "bottle": { "stable": { "files": {} } }
        }"#;

        Mock::given(method("GET"))
            .and(path("/formula/nothing.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(formula_json))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            root.join("locks"),
        );

        let result = installer.plan(&["nothing".to_string()]).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            zb_core::Error::MissingFormula { .. }
        ));
    }

    #[tokio::test]
    async fn plan_best_effort_keeps_valid_formula_when_another_is_missing() {
        let mock_server = MockServer::start().await;
        let tmp = TempDir::new().unwrap();

        let tag = get_test_bottle_tag();
        let formula_json = format!(
            r#"{{
                "name": "goodpkg",
                "versions": {{ "stable": "1.0.0" }},
                "dependencies": [],
                "bottle": {{
                    "stable": {{
                        "files": {{
                            "{}": {{
                                "url": "{}/bottles/goodpkg-1.0.0.{}.bottle.tar.gz",
                                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                            }}
                        }}
                    }}
                }}
            }}"#,
            tag,
            mock_server.uri(),
            tag
        );

        Mock::given(method("GET"))
            .and(path("/formula/goodpkg.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(formula_json))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/formula/missingpkg.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/formula.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&mock_server)
            .await;

        let root = tmp.path().join("zerobrew");
        let prefix = tmp.path().join("homebrew");
        fs::create_dir_all(root.join("db")).unwrap();

        let api_client =
            ApiClient::with_base_url(format!("{}/formula", mock_server.uri())).unwrap();
        let blob_cache = BlobCache::new(&root.join("cache")).unwrap();
        let store = Store::new(&root).unwrap();
        let cellar = Cellar::new(&root).unwrap();
        let linker = Linker::new(&prefix).unwrap();
        let db = Database::open(&root.join("db/zb.sqlite3")).unwrap();

        let installer = Installer::new(
            api_client,
            blob_cache,
            store,
            cellar,
            linker,
            db,
            prefix.clone(),
            root.join("locks"),
        );

        let names = vec!["goodpkg".to_string(), "missingpkg".to_string()];
        let (plan, failures) = installer.plan_best_effort(&names, false).await;

        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].install_name, "goodpkg");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].name, "missingpkg");
        assert!(matches!(
            failures[0].error,
            zb_core::Error::MissingFormula { .. }
        ));
    }
}
