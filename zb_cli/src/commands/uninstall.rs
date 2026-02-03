use console::style;
use std::collections::{BTreeMap, BTreeSet};
use zb_io::DependencyRow;

pub fn execute(
    installer: &mut zb_io::install::Installer,
    formulas: Vec<String>,
    all: bool,
) -> Result<(), zb_core::Error> {
    let formulas = if all {
        let installed = installer.list_installed()?;
        if installed.is_empty() {
            println!("No formulas installed.");
            return Ok(());
        }
        installed.into_iter().map(|k| k.name).collect()
    } else {
        formulas
    };

    let removal_plan = build_removal_plan(installer, &formulas)?;
    if removal_plan.is_empty() {
        println!("No formulas installed.");
        return Ok(());
    }

    let extras: Vec<String> = removal_plan
        .iter()
        .filter(|name| !formulas.contains(name))
        .cloned()
        .collect();

    println!(
        "{} Uninstalling {}...",
        style("==>").cyan().bold(),
        style(removal_plan.join(", ")).bold()
    );
    if !extras.is_empty() {
        println!("    Also removing: {}", extras.join(", "));
    }

    let mut errors: Vec<(String, zb_core::Error)> = Vec::new();

    if removal_plan.len() > 1 {
        for name in &removal_plan {
            print!("    {} {}...", style("○").dim(), name);
            match installer.uninstall(name) {
                Ok(()) => println!(" {}", style("✓").green()),
                Err(e) => {
                    println!(" {}", style("✗").red());
                    errors.push((name.clone(), e));
                }
            }
        }
    } else if let Err(e) = installer.uninstall(&removal_plan[0]) {
        errors.push((removal_plan[0].clone(), e));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        for (name, err) in &errors {
            eprintln!(
                "{} Failed to uninstall {}: {}",
                style("Error:").red().bold(),
                style(name).bold(),
                err
            );
        }
        // Return just the first error up. TODO: don't return errors from this fn?
        Err(errors.remove(0).1)
    }
}

fn build_removal_plan(
    installer: &mut zb_io::install::Installer,
    formulas: &[String],
) -> Result<Vec<String>, zb_core::Error> {
    if formulas.is_empty() {
        return Ok(Vec::new());
    }

    let installed = installer.list_installed()?;
    if installed.is_empty() {
        return Ok(Vec::new());
    }
    let installed_set: BTreeSet<String> = installed.into_iter().map(|k| k.name).collect();

    let dependency_rows = installer.list_dependencies()?;
    if dependency_rows.is_empty() {
        return Ok(formulas
            .iter()
            .filter(|name| installed_set.contains(*name))
            .cloned()
            .collect());
    }

    let explicit_set: BTreeSet<String> = installer
        .list_explicit()?
        .into_iter()
        .filter(|name| installed_set.contains(name))
        .collect();

    let (dependencies, reverse) = build_dependency_maps(&dependency_rows, &installed_set);
    let candidates = build_candidate_set(formulas, &dependencies, &installed_set);

    let removal_set = filter_removal_set(
        formulas,
        &candidates,
        &reverse,
        &explicit_set,
        &installed_set,
    );

    Ok(topo_uninstall_order(&removal_set, &dependencies))
}

fn build_dependency_maps(
    dependency_rows: &[DependencyRow],
    installed_set: &BTreeSet<String>,
) -> (
    BTreeMap<String, BTreeSet<String>>,
    BTreeMap<String, BTreeSet<String>>,
) {
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for row in dependency_rows {
        if !installed_set.contains(&row.name) || !installed_set.contains(&row.dependency) {
            continue;
        }
        dependencies
            .entry(row.name.clone())
            .or_default()
            .insert(row.dependency.clone());
        reverse
            .entry(row.dependency.clone())
            .or_default()
            .insert(row.name.clone());
    }

    (dependencies, reverse)
}

fn build_candidate_set(
    formulas: &[String],
    dependencies: &BTreeMap<String, BTreeSet<String>>,
    installed_set: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = formulas
        .iter()
        .filter(|name| installed_set.contains(*name))
        .cloned()
        .collect();

    while let Some(name) = stack.pop() {
        if !candidates.insert(name.clone()) {
            continue;
        }
        if let Some(deps) = dependencies.get(&name) {
            for dep in deps {
                if installed_set.contains(dep) {
                    stack.push(dep.clone());
                }
            }
        }
    }

    candidates
}

fn filter_removal_set(
    roots: &[String],
    candidates: &BTreeSet<String>,
    reverse: &BTreeMap<String, BTreeSet<String>>,
    explicit_set: &BTreeSet<String>,
    installed_set: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut removal_set: BTreeSet<String> = BTreeSet::new();
    let root_set: BTreeSet<String> = roots.iter().cloned().collect();

    for name in candidates {
        if root_set.contains(name) {
            removal_set.insert(name.clone());
            continue;
        }
        if explicit_set.contains(name) {
            continue;
        }
        if has_remaining_dependents(name, candidates, reverse, installed_set) {
            continue;
        }
        removal_set.insert(name.clone());
    }

    removal_set
}

fn has_remaining_dependents(
    name: &str,
    candidates: &BTreeSet<String>,
    reverse: &BTreeMap<String, BTreeSet<String>>,
    installed_set: &BTreeSet<String>,
) -> bool {
    let Some(dependents) = reverse.get(name) else {
        return false;
    };
    dependents
        .iter()
        .any(|dep| installed_set.contains(dep) && !candidates.contains(dep))
}

fn topo_uninstall_order(
    removal_set: &BTreeSet<String>,
    dependencies: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let mut indegree: BTreeMap<String, usize> =
        removal_set.iter().map(|name| (name.clone(), 0)).collect();

    for name in removal_set {
        if let Some(deps) = dependencies.get(name) {
            for dep in deps {
                if removal_set.contains(dep) {
                    if let Some(count) = indegree.get_mut(dep) {
                        *count += 1;
                    }
                }
            }
        }
    }

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

    let mut ordered = Vec::with_capacity(removal_set.len());
    while let Some(name) = ready.iter().next().cloned() {
        ready.take(&name);
        ordered.push(name.clone());
        if let Some(deps) = dependencies.get(&name) {
            for dep in deps {
                if let Some(count) = indegree.get_mut(dep) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(dep.clone());
                    }
                }
            }
        }
    }

    if ordered.len() == removal_set.len() {
        ordered
    } else {
        removal_set.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn build_candidate_set_collects_transitive_deps() {
        let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        dependencies.insert("a".to_string(), set(&["b", "c"]));
        dependencies.insert("b".to_string(), set(&["d"]));

        let installed_set = set(&["a", "b", "c", "d", "e"]);
        let formulas = vec!["a".to_string()];
        let candidates = build_candidate_set(&formulas, &dependencies, &installed_set);

        assert_eq!(candidates, set(&["a", "b", "c", "d"]));
    }

    #[test]
    fn filter_removal_set_skips_explicit_and_remaining_dependents() {
        let candidates = set(&["a", "b", "c"]);
        let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        reverse.insert("b".to_string(), set(&["a"]));
        reverse.insert("c".to_string(), set(&["a", "d"]));

        let explicit_set = set(&["b"]);
        let installed_set = set(&["a", "b", "c", "d"]);
        let roots = vec!["a".to_string()];

        let removal =
            filter_removal_set(&roots, &candidates, &reverse, &explicit_set, &installed_set);
        assert_eq!(removal, set(&["a"]));
    }

    #[test]
    fn topo_uninstall_order_removes_dependents_first() {
        let removal_set = set(&["a", "b", "c"]);
        let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        dependencies.insert("a".to_string(), set(&["b"]));
        dependencies.insert("b".to_string(), set(&["c"]));

        let ordered = topo_uninstall_order(&removal_set, &dependencies);
        assert_eq!(
            ordered,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }
}
