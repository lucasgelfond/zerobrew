use crate::{Error, Formula};
use std::collections::{BTreeMap, BTreeSet};

type InDegreeMap = BTreeMap<String, usize>;
type AdjacencyMap = BTreeMap<String, BTreeSet<String>>;

pub fn resolve_closure(
    roots: &[String],
    formulas: &BTreeMap<String, Formula>,
) -> Result<Vec<String>, Error> {
    let closure = compute_closure(roots, formulas)?;
    let (mut indegree, adjacency) = build_graph(&closure, formulas)?;

    let mut ready: BTreeSet<String> = indegree
        .iter()
        .filter_map(|(name, count)| {
            if *count == 0 {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    let mut ordered = Vec::with_capacity(closure.len());
    while let Some(name) = ready.iter().next().cloned() {
        ready.take(&name);
        ordered.push(name.clone());
        if let Some(children) = adjacency.get(&name) {
            for child in children {
                if let Some(count) = indegree.get_mut(child) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(child.clone());
                    }
                }
            }
        }
    }

    if ordered.len() != closure.len() {
        let cycle: Vec<String> = indegree
            .into_iter()
            .filter_map(|(name, count)| if count > 0 { Some(name) } else { None })
            .collect();
        return Err(Error::DependencyCycle { cycle });
    }

    Ok(ordered)
}

fn compute_closure(
    roots: &[String],
    formulas: &BTreeMap<String, Formula>,
) -> Result<BTreeSet<String>, Error> {
    let mut closure = BTreeSet::new();
    let mut stack = roots.to_vec();

    while let Some(name) = stack.pop() {
        if !closure.insert(name.clone()) {
            continue;
        }

        let formula = formulas
            .get(&name)
            .ok_or_else(|| Error::MissingFormula { name: name.clone() })?;

        let mut deps = formula.dependencies.clone();
        deps.sort();
        for dep in deps {
            // Skip dependencies that aren't in the formulas map
            // (they were filtered out due to missing bottles for this platform)
            // FIXME: we should probably just ignore these dependencies instead of skipping the entire formula
            if !formulas.contains_key(&dep) {
                continue;
            }
            if !closure.contains(&dep) {
                stack.push(dep);
            }
        }
    }

    Ok(closure)
}

fn build_graph(
    closure: &BTreeSet<String>,
    formulas: &BTreeMap<String, Formula>,
) -> Result<(InDegreeMap, AdjacencyMap), Error> {
    let mut indegree: InDegreeMap = closure.iter().map(|name| (name.clone(), 0)).collect();
    let mut adjacency: AdjacencyMap = BTreeMap::new();

    for name in closure {
        let formula = formulas
            .get(name)
            .ok_or_else(|| Error::MissingFormula { name: name.clone() })?;
        let mut deps = formula.dependencies.clone();
        deps.sort();
        for dep in deps {
            if !closure.contains(&dep) {
                continue;
            }
            if let Some(count) = indegree.get_mut(name) {
                *count += 1;
            }
            adjacency.entry(dep).or_default().insert(name.clone());
        }
    }

    Ok((indegree, adjacency))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{Bottle, BottleFile, BottleStable, Versions};
    use std::collections::BTreeMap;

    fn formula(name: &str, deps: &[&str]) -> Formula {
        let mut files = BTreeMap::new();
        files.insert(
            "arm64_sonoma".to_string(),
            BottleFile {
                url: format!("https://example.com/{name}.tar.gz"),
                sha256: "deadbeef".repeat(8),
            },
        );

        Formula {
            name: name.to_string(),
            versions: Versions {
                stable: "1.0.0".to_string(),
            },
            dependencies: deps.iter().map(|dep| dep.to_string()).collect(),
            bottle: Bottle {
                stable: BottleStable { files, rebuild: 0 },
            },
            revision: 0,
        }
    }

    #[test]
    fn resolves_transitive_closure_in_stable_order() {
        let mut formulas = BTreeMap::new();
        formulas.insert("foo".to_string(), formula("foo", &["baz", "bar"]));
        formulas.insert("bar".to_string(), formula("bar", &["qux"]));
        formulas.insert("baz".to_string(), formula("baz", &["qux"]));
        formulas.insert("qux".to_string(), formula("qux", &[]));

        let order = resolve_closure(&["foo".to_string()], &formulas).unwrap();
        assert_eq!(order, vec!["qux", "bar", "baz", "foo"]);
    }

    #[test]
    fn resolves_multiple_roots_with_shared_deps() {
        let mut formulas = BTreeMap::new();
        formulas.insert("a".to_string(), formula("a", &["shared"]));
        formulas.insert("b".to_string(), formula("b", &["shared"]));
        formulas.insert("shared".to_string(), formula("shared", &[]));

        let order = resolve_closure(&["a".to_string(), "b".to_string()], &formulas).unwrap();
        // shared should come first, then a and b in stable order
        assert_eq!(order, vec!["shared", "a", "b"]);
    }

    #[test]
    fn detects_cycles() {
        let mut formulas = BTreeMap::new();
        formulas.insert("alpha".to_string(), formula("alpha", &["beta"]));
        formulas.insert("beta".to_string(), formula("beta", &["gamma"]));
        formulas.insert("gamma".to_string(), formula("gamma", &["alpha"]));

        let err = resolve_closure(&["alpha".to_string()], &formulas).unwrap_err();
        assert!(matches!(err, Error::DependencyCycle { .. }));
    }

    #[test]
    fn skips_missing_dependencies() {
        // Test that dependencies not in the formulas map are skipped
        // (e.g., platform-incompatible dependencies filtered out during fetch)
        let mut formulas = BTreeMap::new();
        formulas.insert("git".to_string(), formula("git", &["gettext", "libiconv"]));
        formulas.insert("gettext".to_string(), formula("gettext", &[]));
        // libiconv is intentionally missing (filtered out for Linux)

        let order = resolve_closure(&["git".to_string()], &formulas).unwrap();
        // Should successfully resolve with just git and gettext
        assert_eq!(order, vec!["gettext", "git"]);
    }
}
