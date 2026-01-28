mod common;

use common::formula;
use std::collections::BTreeMap;
use zb_core::{resolve_closure, Error};

#[test]
fn simple_two_node_cycle() {
    let mut formulas = BTreeMap::new();
    formulas.insert("alpha".to_string(), formula("alpha", "1.0.0", &["beta"]));
    formulas.insert("beta".to_string(), formula("beta", "1.0.0", &["alpha"]));

    let result = resolve_closure("alpha", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            assert_eq!(cycle.len(), 2);
            assert!(cycle.contains(&"alpha".to_string()));
            assert!(cycle.contains(&"beta".to_string()));
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn three_node_cycle() {
    let mut formulas = BTreeMap::new();
    formulas.insert("alpha".to_string(), formula("alpha", "1.0.0", &["beta"]));
    formulas.insert("beta".to_string(), formula("beta", "1.0.0", &["gamma"]));
    formulas.insert("gamma".to_string(), formula("gamma", "1.0.0", &["alpha"]));

    let result = resolve_closure("alpha", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
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
fn self_dependency_cycle() {
    let mut formulas = BTreeMap::new();
    formulas.insert("foo".to_string(), formula("foo", "1.0.0", &["foo"]));

    let result = resolve_closure("foo", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            assert!(cycle.contains(&"foo".to_string()));
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn cycle_deep_in_graph() {
    // Complex graph where the cycle is not at the root:
    // root -> a -> b -> c -> d
    //              ^         |
    //              |---------|
    let mut formulas = BTreeMap::new();
    formulas.insert("root".to_string(), formula("root", "1.0.0", &["a"]));
    formulas.insert("a".to_string(), formula("a", "1.0.0", &["b"]));
    formulas.insert("b".to_string(), formula("b", "1.0.0", &["c"]));
    formulas.insert("c".to_string(), formula("c", "1.0.0", &["d"]));
    formulas.insert("d".to_string(), formula("d", "1.0.0", &["b"]));

    let result = resolve_closure("root", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            // The cycle should contain b, c, d
            assert!(cycle.contains(&"b".to_string()));
            assert!(cycle.contains(&"c".to_string()));
            assert!(cycle.contains(&"d".to_string()));
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn multiple_cycles_detects_at_least_one() {
    // Graph with multiple disconnected cycles
    // a -> b -> a (cycle 1)
    // c -> d -> c (cycle 2)
    let mut formulas = BTreeMap::new();
    formulas.insert("a".to_string(), formula("a", "1.0.0", &["b", "c"]));
    formulas.insert("b".to_string(), formula("b", "1.0.0", &["a"]));
    formulas.insert("c".to_string(), formula("c", "1.0.0", &["d"]));
    formulas.insert("d".to_string(), formula("d", "1.0.0", &["c"]));

    let result = resolve_closure("a", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            // Should detect at least one cycle
            assert!(!cycle.is_empty());
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn diamond_graph_no_cycle() {
    // Diamond dependency graph should NOT be detected as a cycle:
    // root -> a -> c
    //      -> b -> c
    let mut formulas = BTreeMap::new();
    formulas.insert("root".to_string(), formula("root", "1.0.0", &["a", "b"]));
    formulas.insert("a".to_string(), formula("a", "1.0.0", &["c"]));
    formulas.insert("b".to_string(), formula("b", "1.0.0", &["c"]));
    formulas.insert("c".to_string(), formula("c", "1.0.0", &[]));

    let result = resolve_closure("root", &formulas);
    assert!(result.is_ok());
    let order = result.unwrap();
    // c should come before a and b, which should come before root
    let c_pos = order.iter().position(|n| n == "c").unwrap();
    let a_pos = order.iter().position(|n| n == "a").unwrap();
    let b_pos = order.iter().position(|n| n == "b").unwrap();
    let root_pos = order.iter().position(|n| n == "root").unwrap();
    assert!(c_pos < a_pos);
    assert!(c_pos < b_pos);
    assert!(a_pos < root_pos);
    assert!(b_pos < root_pos);
}

#[test]
fn long_chain_with_cycle_at_end() {
    // Long chain with a cycle at the end:
    // a -> b -> c -> d -> e -> f -> d (cycle back to d)
    let mut formulas = BTreeMap::new();
    formulas.insert("a".to_string(), formula("a", "1.0.0", &["b"]));
    formulas.insert("b".to_string(), formula("b", "1.0.0", &["c"]));
    formulas.insert("c".to_string(), formula("c", "1.0.0", &["d"]));
    formulas.insert("d".to_string(), formula("d", "1.0.0", &["e"]));
    formulas.insert("e".to_string(), formula("e", "1.0.0", &["f"]));
    formulas.insert("f".to_string(), formula("f", "1.0.0", &["d"]));

    let result = resolve_closure("a", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            // The cycle involves d, e, f
            assert!(cycle.contains(&"d".to_string()));
            assert!(cycle.contains(&"e".to_string()));
            assert!(cycle.contains(&"f".to_string()));
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn multiple_paths_to_cycle() {
    // Multiple paths leading to the same cycle:
    // root -> a -> c -> d -> c (cycle)
    //      -> b -> c
    let mut formulas = BTreeMap::new();
    formulas.insert("root".to_string(), formula("root", "1.0.0", &["a", "b"]));
    formulas.insert("a".to_string(), formula("a", "1.0.0", &["c"]));
    formulas.insert("b".to_string(), formula("b", "1.0.0", &["c"]));
    formulas.insert("c".to_string(), formula("c", "1.0.0", &["d"]));
    formulas.insert("d".to_string(), formula("d", "1.0.0", &["c"]));

    let result = resolve_closure("root", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            assert!(cycle.contains(&"c".to_string()));
            assert!(cycle.contains(&"d".to_string()));
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn cycle_with_sorted_dependencies() {
    // Ensures that dependency sorting doesn't affect cycle detection
    let mut formulas = BTreeMap::new();
    formulas.insert(
        "zebra".to_string(),
        formula("zebra", "1.0.0", &["alpha", "beta"]),
    );
    formulas.insert("alpha".to_string(), formula("alpha", "1.0.0", &["beta"]));
    formulas.insert("beta".to_string(), formula("beta", "1.0.0", &["zebra"]));

    let result = resolve_closure("zebra", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            assert!(cycle.contains(&"zebra".to_string()));
            assert!(cycle.contains(&"beta".to_string()));
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn large_cycle() {
    // 10 nodes here
    let mut formulas = BTreeMap::new();
    let nodes = vec!["n0", "n1", "n2", "n3", "n4", "n5", "n6", "n7", "n8", "n9"];

    for i in 0..nodes.len() {
        let next = nodes[(i + 1) % nodes.len()];
        formulas.insert(
            nodes[i].to_string(),
            formula(nodes[i], "1.0.0", &[next]),
        );
    }

    let result = resolve_closure("n0", &formulas);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::DependencyCycle { cycle } => {
            assert_eq!(cycle.len(), 10);
            for node in &nodes {
                assert!(cycle.contains(&node.to_string()));
            }
        }
        _ => panic!("Expected DependencyCycle error"),
    }
}

#[test]
fn no_cycle_in_complex_dag() {
    // Complex DAG without cycles to ensure we don't have false positives
    // root -> [a, b, c]
    // a -> [d, e]
    // b -> [e, f]
    // c -> [f]
    // d -> [g]
    // e -> [g]
    // f -> [g]
    // g -> []
    let mut formulas = BTreeMap::new();
    formulas.insert("root".to_string(), formula("root", "1.0.0", &["a", "b", "c"]));
    formulas.insert("a".to_string(), formula("a", "1.0.0", &["d", "e"]));
    formulas.insert("b".to_string(), formula("b", "1.0.0", &["e", "f"]));
    formulas.insert("c".to_string(), formula("c", "1.0.0", &["f"]));
    formulas.insert("d".to_string(), formula("d", "1.0.0", &["g"]));
    formulas.insert("e".to_string(), formula("e", "1.0.0", &["g"]));
    formulas.insert("f".to_string(), formula("f", "1.0.0", &["g"]));
    formulas.insert("g".to_string(), formula("g", "1.0.0", &[]));

    let result = resolve_closure("root", &formulas);
    assert!(result.is_ok());
    let order = result.unwrap();
    assert_eq!(order.len(), 8);

    // g should be first (no dependencies)
    assert_eq!(order[0], "g");
    // root should be last (depends on everything)
    assert_eq!(order[7], "root");
}
