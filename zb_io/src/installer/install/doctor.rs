use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use zb_core::{Error, formula_token};

use crate::storage::db::StoreRef;

use super::Installer;

#[derive(Debug, Default)]
pub struct DiagnosticReport {
    pub orphaned_cellar_kegs: Vec<OrphanedKeg>,
    pub missing_cellar_kegs: Vec<MissingKeg>,
    pub orphaned_store_entries: Vec<String>,
    pub stale_store_refs: Vec<StaleStoreRef>,
    pub broken_symlinks: Vec<PathBuf>,
    pub stale_keg_file_records: usize,
}

#[derive(Debug)]
pub struct OrphanedKeg {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct MissingKeg {
    pub name: String,
    pub version: String,
    pub expected_path: PathBuf,
}

#[derive(Debug)]
pub struct StaleStoreRef {
    pub store_key: String,
    pub refcount: i64,
    pub on_disk: bool,
    pub referenced_by_any_keg: bool,
}

impl DiagnosticReport {
    pub fn is_healthy(&self) -> bool {
        self.orphaned_cellar_kegs.is_empty()
            && self.missing_cellar_kegs.is_empty()
            && self.orphaned_store_entries.is_empty()
            && self.stale_store_refs.is_empty()
            && self.broken_symlinks.is_empty()
            && self.stale_keg_file_records == 0
    }
}

impl Installer {
    pub fn doctor(&mut self) -> Result<DiagnosticReport, Error> {
        let mut report = DiagnosticReport::default();

        let installed = self.db.list_installed()?;
        let db_store_refs = self.db.list_store_refs()?;
        let disk_store_entries = self.store.list_entries()?;
        let cellar_kegs = self.cellar.list_kegs()?;

        let installed_by_token: HashMap<&str, &crate::storage::db::InstalledKeg> = installed
            .iter()
            .map(|k| (formula_token(&k.name), k))
            .collect();

        for keg in &cellar_kegs {
            if !installed_by_token.contains_key(keg.name.as_str()) {
                report.orphaned_cellar_kegs.push(OrphanedKeg {
                    name: keg.name.clone(),
                    version: keg.version.clone(),
                    path: keg.path.clone(),
                });
            }
        }

        for keg in &installed {
            let token = formula_token(&keg.name);
            let expected_path = self.cellar.keg_path(token, &keg.version);
            if !expected_path.exists() {
                report.missing_cellar_kegs.push(MissingKeg {
                    name: keg.name.clone(),
                    version: keg.version.clone(),
                    expected_path,
                });
            }
        }

        let store_keys_in_db: HashSet<&str> =
            db_store_refs.iter().map(|r| r.store_key.as_str()).collect();

        let disk_store_set: HashSet<&str> = disk_store_entries.iter().map(String::as_str).collect();

        let store_keys_used: HashMap<&str, i64> = {
            let mut map = HashMap::new();
            for keg in &installed {
                *map.entry(keg.store_key.as_str()).or_insert(0) += 1;
            }
            map
        };

        for entry in &disk_store_entries {
            if !store_keys_in_db.contains(entry.as_str()) {
                report.orphaned_store_entries.push(entry.clone());
            }
        }

        for store_ref in &db_store_refs {
            let actual_count = store_keys_used
                .get(store_ref.store_key.as_str())
                .copied()
                .unwrap_or(0);
            let on_disk = disk_store_set.contains(store_ref.store_key.as_str());

            if store_ref.refcount != actual_count || !on_disk {
                report.stale_store_refs.push(StaleStoreRef {
                    store_key: store_ref.store_key.clone(),
                    refcount: store_ref.refcount,
                    on_disk,
                    referenced_by_any_keg: actual_count > 0,
                });
            }
        }

        for keg in &installed {
            let token = formula_token(&keg.name);
            let keg_path = self.cellar.keg_path(token, &keg.version);
            if !keg_path.exists() {
                continue;
            }
            let linked = self.linker.collect_linked_files(&keg_path)?;
            for file in linked {
                if !file.target_path.exists() {
                    report.broken_symlinks.push(file.link_path);
                }
            }
        }

        report.stale_keg_file_records = self.db.count_stale_keg_file_records()?;

        Ok(report)
    }

    pub fn repair(&mut self, report: &DiagnosticReport) -> Result<RepairSummary, Error> {
        let mut summary = RepairSummary::default();

        for orphan in &report.orphaned_cellar_kegs {
            self.linker.unlink_keg(&orphan.path).ok();
            self.cellar.remove_keg(&orphan.name, &orphan.version)?;
            summary.removed_orphaned_kegs += 1;
        }

        for missing in &report.missing_cellar_kegs {
            let tx = self.db.transaction()?;
            tx.delete_installed_record(&missing.name)?;
            tx.commit()?;
            summary.removed_missing_records += 1;
        }

        if !report.stale_store_refs.is_empty() {
            let installed = self.db.list_installed()?;
            let mut corrected: HashMap<&str, i64> = HashMap::new();
            for keg in &installed {
                *corrected.entry(keg.store_key.as_str()).or_insert(0) += 1;
            }

            let corrected_refs: Vec<StoreRef> = corrected
                .into_iter()
                .map(|(store_key, refcount)| StoreRef {
                    store_key: store_key.to_owned(),
                    refcount,
                })
                .collect();

            self.db.replace_store_refs(&corrected_refs)?;
            summary.fixed_store_refs = report.stale_store_refs.len();
        }

        for key in &report.orphaned_store_entries {
            self.store.remove_entry(key)?;
            summary.removed_orphaned_store_entries += 1;
        }

        for link in &report.broken_symlinks {
            let _ = std::fs::remove_file(link);
            summary.removed_broken_symlinks += 1;
        }

        if report.stale_keg_file_records > 0 {
            summary.pruned_keg_file_records = self.db.prune_stale_keg_file_records()?;
        }

        Ok(summary)
    }
}

#[derive(Debug, Default)]
pub struct RepairSummary {
    pub removed_orphaned_kegs: usize,
    pub removed_missing_records: usize,
    pub fixed_store_refs: usize,
    pub removed_orphaned_store_entries: usize,
    pub removed_broken_symlinks: usize,
    pub pruned_keg_file_records: usize,
}

impl RepairSummary {
    pub fn total_fixes(&self) -> usize {
        self.removed_orphaned_kegs
            + self.removed_missing_records
            + self.fixed_store_refs
            + self.removed_orphaned_store_entries
            + self.removed_broken_symlinks
            + self.pruned_keg_file_records
    }
}
