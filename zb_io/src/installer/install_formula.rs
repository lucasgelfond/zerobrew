use std::collections::{BTreeMap, HashMap, HashSet};

use crate::network::api::ApiClient;
use crate::storage::db::Database;
use crate::tap::{TapRef, fetch_formula as fetch_tap_formula};
use zb_core::{Error, Formula};

#[derive(Clone, Debug)]
pub(crate) struct FormulaRef {
    pub(crate) name: String,
    pub(crate) tap: Option<TapRef>,
    pub(crate) exact_tap: bool,
}

#[derive(Clone, Debug)]
struct FormulaRequest {
    name: String,
    preferred_tap: Option<TapRef>,
    exact_tap: bool,
}

pub fn parse_formula_ref(input: &str) -> Result<FormulaRef, Error> {
    let parts: Vec<&str> = input.split('/').collect();
    match parts.len() {
        1 => Ok(FormulaRef {
            name: parts[0].to_string(),
            tap: None,
            exact_tap: false,
        }),
        3 => {
            if parts.iter().any(|p| p.is_empty()) {
                return Err(Error::InvalidFormulaRef {
                    reference: input.to_string(),
                });
            }

            Ok(FormulaRef {
                name: parts[2].to_string(),
                tap: Some(TapRef {
                    owner: parts[0].to_string(),
                    repo: parts[1].to_string(),
                }),
                exact_tap: true,
            })
        }
        _ => Err(Error::InvalidFormulaRef {
            reference: input.to_string(),
        }),
    }
}

fn source_label(source: Option<&TapRef>) -> String {
    match source {
        Some(tap) => format!("tap {}", tap.label()),
        None => "core".to_string(),
    }
}

pub struct FormulaResolver<'a> {
    api_client: &'a ApiClient,
    db: &'a Database,
}

impl<'a> FormulaResolver<'a> {
    pub(crate) fn new(api_client: &'a ApiClient, db: &'a Database) -> Self {
        Self { api_client, db }
    }

    /// Recursively fetch formulas and all their dependencies in parallel batches.
    pub(crate) async fn fetch_all_formulas(
        &self,
        names: &[String],
    ) -> Result<BTreeMap<String, Formula>, Error> {
        let mut formulas = BTreeMap::new();
        let mut fetched: HashSet<String> = HashSet::new();
        let mut sources: HashMap<String, Option<TapRef>> = HashMap::new();

        let taps: Vec<TapRef> = self
            .db
            .list_taps()?
            .into_iter()
            .map(|tap| TapRef {
                owner: tap.owner,
                repo: tap.repo,
            })
            .collect();

        let mut to_fetch: Vec<FormulaRequest> = Vec::new();
        for name in names {
            let root_ref = parse_formula_ref(name)?;
            let next_request = FormulaRequest {
                name: root_ref.name,
                preferred_tap: root_ref.tap,
                exact_tap: root_ref.exact_tap,
            };

            if let Some(existing) = to_fetch.iter().find(|r| r.name == next_request.name) {
                if existing.preferred_tap != next_request.preferred_tap
                    || existing.exact_tap != next_request.exact_tap
                {
                    return Err(Error::ConflictingFormulaSource {
                        name: next_request.name,
                        first: source_label(existing.preferred_tap.as_ref()),
                        second: source_label(next_request.preferred_tap.as_ref()),
                    });
                }
                continue;
            }

            to_fetch.push(next_request);
        }

        while !to_fetch.is_empty() {
            // Fetch current batch in parallel
            let batch: Vec<FormulaRequest> = to_fetch
                .drain(..)
                .filter(|r| !fetched.contains(&r.name))
                .collect();

            if batch.is_empty() {
                break;
            }

            // Mark as fetched before starting (to avoid re-queueing)
            for req in &batch {
                fetched.insert(req.name.clone());
            }

            // Fetch all in parallel
            let futures: Vec<_> = batch
                .iter()
                .map(|req| {
                    self.fetch_formula_with_resolution(
                        &req.name,
                        req.preferred_tap.as_ref(),
                        req.exact_tap,
                        &taps,
                    )
                })
                .collect();

            let results = futures::future::join_all(futures).await;

            // Process results and queue new dependencies
            for (i, result) in results.into_iter().enumerate() {
                let (formula, source) = result?;
                let name = batch[i].name.clone();

                if let Some(existing) = sources.get(&name)
                    && existing != &source
                {
                    return Err(Error::ConflictingFormulaSource {
                        name,
                        first: source_label(existing.as_ref()),
                        second: source_label(source.as_ref()),
                    });
                }

                sources.insert(name.clone(), source.clone());

                // Queue dependencies for next batch
                for dep in &formula.dependencies {
                    let dep_ref = parse_formula_ref(dep)?;
                    let next_request = FormulaRequest {
                        name: dep_ref.name,
                        preferred_tap: dep_ref.tap.or_else(|| source.clone()),
                        exact_tap: dep_ref.exact_tap,
                    };

                    if let Some(existing) = sources.get(&next_request.name) {
                        if existing != &next_request.preferred_tap {
                            return Err(Error::ConflictingFormulaSource {
                                name: next_request.name.clone(),
                                first: source_label(existing.as_ref()),
                                second: source_label(next_request.preferred_tap.as_ref()),
                            });
                        }
                        continue;
                    }

                    if fetched.contains(&next_request.name) {
                        continue;
                    }

                    if let Some(existing) = to_fetch.iter().find(|r| r.name == next_request.name) {
                        if existing.preferred_tap != next_request.preferred_tap
                            || existing.exact_tap != next_request.exact_tap
                        {
                            return Err(Error::ConflictingFormulaSource {
                                name: next_request.name.clone(),
                                first: source_label(existing.preferred_tap.as_ref()),
                                second: source_label(next_request.preferred_tap.as_ref()),
                            });
                        }

                        continue;
                    }

                    to_fetch.push(next_request);
                }

                formulas.insert(name, formula);
            }
        }

        Ok(formulas)
    }

    async fn fetch_formula_with_resolution(
        &self,
        name: &str,
        preferred_tap: Option<&TapRef>,
        exact_tap: bool,
        taps: &[TapRef],
    ) -> Result<(Formula, Option<TapRef>), Error> {
        if exact_tap && let Some(tap) = preferred_tap {
            match fetch_tap_formula(self.api_client, tap, name).await {
                Ok(formula) => return Ok((formula, Some(tap.clone()))),
                Err(Error::MissingFormula { .. }) => {
                    return Err(Error::MissingFormulaInSources {
                        name: name.to_string(),
                        sources: vec![source_label(Some(tap))],
                    });
                }
                Err(e) => return Err(e),
            };
        }

        let core_base = self.api_client.base_url();
        let mut candidates: Vec<Option<TapRef>> = Vec::new();
        if let Some(tap) = preferred_tap {
            candidates.push(Some(tap.clone()));
        }
        for tap in taps {
            if preferred_tap.map(|p| p != tap).unwrap_or(true) {
                candidates.push(Some(tap.clone()));
            }
        }
        candidates.push(None);

        let mut tried_sources = Vec::new();
        for source in candidates {
            match &source {
                Some(tap) => match fetch_tap_formula(self.api_client, tap, name).await {
                    Ok(formula) => return Ok((formula, Some(tap.clone()))),
                    Err(Error::MissingFormula { .. }) => {
                        tried_sources.push(source_label(Some(tap)));
                        continue;
                    }
                    Err(e) => return Err(e),
                },
                None => {
                    let base_url = core_base.to_string();
                    match self
                        .api_client
                        .get_formula_with_base_url(&base_url, name)
                        .await
                    {
                        Ok(formula) => return Ok((formula, None)),
                        Err(Error::MissingFormula { .. }) => {
                            tried_sources.push(source_label(None));
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
            };
        }

        Err(Error::MissingFormulaInSources {
            name: name.to_string(),
            sources: tried_sources,
        })
    }
}
