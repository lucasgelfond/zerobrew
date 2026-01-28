use crate::{Error, Formula};
use std::collections::{BTreeMap, BTreeSet};

type InDegreeMap = BTreeMap<String, usize>;
type AdjacencyMap = BTreeMap<String, BTreeSet<String>>;

pub fn resolve_closure(
    root: &str,
    formulas: &BTreeMap<String, Formula>,
) -> Result<Vec<String>, Error> {
    let closure = compute_closure(root, formulas)?;
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
    root: &str,
    formulas: &BTreeMap<String, Formula>,
) -> Result<BTreeSet<String>, Error> {
    let mut closure = BTreeSet::new();
    let mut stack = vec![root.to_string()];

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
        }
    }

    #[test]
    fn resolves_transitive_closure_in_stable_order() {
        let mut formulas = BTreeMap::new();
        formulas.insert("foo".to_string(), formula("foo", &["baz", "bar"]));
        formulas.insert("bar".to_string(), formula("bar", &["qux"]));
        formulas.insert("baz".to_string(), formula("baz", &["qux"]));
        formulas.insert("qux".to_string(), formula("qux", &[]));

        let order = resolve_closure("foo", &formulas).unwrap();
        assert_eq!(order, vec!["qux", "bar", "baz", "foo"]);
    }

    #[test]
    fn detects_three_node_cycle() {
        let mut formulas = BTreeMap::new();
        formulas.insert("alpha".to_string(), formula("alpha", &["beta"]));
        formulas.insert("beta".to_string(), formula("beta", &["gamma"]));
        formulas.insert("gamma".to_string(), formula("gamma", &["alpha"]));

        let err = resolve_closure("alpha", &formulas).unwrap_err();
        match err {
            Error::DependencyCycle { cycle } => {
                assert_eq!(cycle.len(), 3);
                assert!(cycle.contains(&"alpha".to_string()));
                assert!(cycle.contains(&"beta".to_string()));
                assert!(cycle.contains(&"gamma".to_string()));
            }
            _ => panic!("Expected DependencyCycle error"),
        }
    }

    #[test]
    fn detects_simple_two_node_cycle() {
        let mut formulas = BTreeMap::new();
        formulas.insert("a".to_string(), formula("a", &["b"]));
        formulas.insert("b".to_string(), formula("b", &["a"]));

        let err = resolve_closure("a", &formulas).unwrap_err();
        assert!(matches!(err, Error::DependencyCycle { .. }));
    }

    #[test]
    fn detects_self_cycle() {
        let mut formulas = BTreeMap::new();
        formulas.insert("loop".to_string(), formula("loop", &["loop"]));

        let err = resolve_closure("loop", &formulas).unwrap_err();
        assert!(matches!(err, Error::DependencyCycle { .. }));
    }

    #[test]
    fn missing_formula_error() {
        let mut formulas = BTreeMap::new();
        formulas.insert("root".to_string(), formula("root", &["missing"]));

        let err = resolve_closure("root", &formulas).unwrap_err();
        match err {
            Error::MissingFormula { name } => {
                assert_eq!(name, "missing");
            }
            _ => panic!("Expected MissingFormula error"),
        }
    }

    #[test]
    fn diamond_dependency_convergence() {
        // Diamond: root -> [a, b] -> c
        let mut formulas = BTreeMap::new();
        formulas.insert("root".to_string(), formula("root", &["a", "b"]));
        formulas.insert("a".to_string(), formula("a", &["c"]));
        formulas.insert("b".to_string(), formula("b", &["c"]));
        formulas.insert("c".to_string(), formula("c", &[]));

        let order = resolve_closure("root", &formulas).unwrap();
        assert_eq!(order.len(), 4);
        // c should come first (no deps), root should be last
        assert_eq!(order[0], "c");
        assert_eq!(order[3], "root");
        // a and b should be in the middle (order between them is deterministic)
        assert!(order[1] == "a" || order[1] == "b");
        assert!(order[2] == "a" || order[2] == "b");
    }

    #[test]
    fn empty_dependencies() {
        let mut formulas = BTreeMap::new();
        formulas.insert("standalone".to_string(), formula("standalone", &[]));

        let order = resolve_closure("standalone", &formulas).unwrap();
        assert_eq!(order, vec!["standalone"]);
    }
}
