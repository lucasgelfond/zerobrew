use console::style;
use std::collections::{BTreeMap, BTreeSet};
use zb_io::DependencyRow;

pub fn execute(installer: &mut zb_io::install::Installer) -> Result<(), zb_core::Error> {
    let installed = installer.list_installed()?;

    if installed.is_empty() {
        println!("No formulas installed.");
    } else {
        let installed_names: Vec<String> = installed.iter().map(|keg| keg.name.clone()).collect();
        let installed_set: BTreeSet<String> = installed_names.iter().cloned().collect();

        let dependency_rows = installer.list_dependencies()?;
        let explicit_rows = installer.list_explicit()?;

        let (dependencies, direct_dependents) =
            build_dependency_maps(&dependency_rows, &installed_set);

        let explicit_set: BTreeSet<String> = explicit_rows
            .into_iter()
            .filter(|name| installed_set.contains(name))
            .collect();
        let has_dependency_data = !dependencies.is_empty();
        let use_explicit = !explicit_set.is_empty();
        let has_metadata = has_dependency_data || use_explicit;

        let roots: BTreeSet<String> = if use_explicit {
            explicit_set.clone()
        } else if has_dependency_data {
            installed_set
                .iter()
                .filter(|name| !direct_dependents.contains_key(*name))
                .cloned()
                .collect()
        } else {
            BTreeSet::new()
        };
        let required_by_roots = if !roots.is_empty() {
            build_required_by_roots(&dependencies, &roots)
        } else {
            BTreeMap::new()
        };

        if !has_metadata {
            eprintln!(
                "note: dependency metadata not available locally yet; reinstall packages to populate"
            );
        }

        for keg in installed {
            let mut line = format!("{} {}", style(&keg.name).bold(), style(&keg.version).dim());

            if has_metadata {
                if let Some(roots) = required_by_roots.get(&keg.name) {
                    let label = format!("(required by: {})", join_names(roots));
                    line.push_str(&format!(" {}", style(label).dim()));
                } else if roots.contains(&keg.name) {
                    let label = if use_explicit { "(explicit)" } else { "(top-level)" };
                    line.push_str(&format!(" {}", style(label).dim()));
                } else if has_dependency_data
                    && let Some(dependents) = direct_dependents.get(&keg.name)
                {
                    let label = format!("(required by: {})", join_names(dependents));
                    line.push_str(&format!(" {}", style(label).dim()));
                }
            }

            println!("{line}");
        }
    }

    Ok(())
}

fn build_dependency_maps(
    dependency_rows: &[DependencyRow],
    installed_set: &BTreeSet<String>,
) -> (BTreeMap<String, BTreeSet<String>>, BTreeMap<String, BTreeSet<String>>) {
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut direct_dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for row in dependency_rows {
        if !installed_set.contains(&row.name) || !installed_set.contains(&row.dependency) {
            continue;
        }
        dependencies
            .entry(row.name.clone())
            .or_default()
            .insert(row.dependency.clone());
        direct_dependents
            .entry(row.dependency.clone())
            .or_default()
            .insert(row.name.clone());
    }

    (dependencies, direct_dependents)
}

fn build_required_by_roots(
    dependencies: &BTreeMap<String, BTreeSet<String>>,
    roots: &BTreeSet<String>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut required_by_roots: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for root in roots {
        let mut stack = vec![root.clone()];
        let mut visited: BTreeSet<String> = BTreeSet::new();

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if current != *root {
                required_by_roots
                    .entry(current.clone())
                    .or_default()
                    .insert(root.clone());
            }
            if let Some(deps) = dependencies.get(&current) {
                for dep in deps {
                    stack.push(dep.clone());
                }
            }
        }
    }

    required_by_roots
}

fn join_names(names: &BTreeSet<String>) -> String {
    names.iter().cloned().collect::<Vec<_>>().join(", ")
}
