use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Formula {
    pub name: String,
    pub versions: Versions,
    pub dependencies: Vec<String>,
    pub bottle: Bottle,
    #[serde(default)]
    pub revision: u32,
}

impl Formula {
    /// Returns the effective version including revision suffix if applicable.
    /// Homebrew formulas with revision > 0 have paths like `{version}_{revision}`.
    /// Note: `rebuild` (in bottle) does NOT affect the installation directory, only the bottle filename.
    pub fn effective_version(&self) -> String {
        if self.revision > 0 {
            format!("{}_{}", self.versions.stable, self.revision)
        } else {
            self.versions.stable.clone()
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Versions {
    pub stable: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Bottle {
    pub stable: BottleStable,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BottleStable {
    pub files: BTreeMap<String, BottleFile>,
    /// Rebuild number for the bottle. When > 0, the bottle's internal paths
    /// use `{version}_{rebuild}` instead of just `{version}`.
    #[serde(default)]
    pub rebuild: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BottleFile {
    pub url: String,
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_formula_fixtures() {
        let fixtures = [
            include_str!("../fixtures/formula_foo.json"),
            include_str!("../fixtures/formula_bar.json"),
        ];

        for fixture in fixtures {
            let formula: Formula = serde_json::from_str(fixture).unwrap();
            assert!(!formula.name.is_empty());
            assert!(!formula.versions.stable.is_empty());
            assert!(!formula.bottle.stable.files.is_empty());
        }
    }

    #[test]
    fn effective_version_without_revision() {
        let fixture = include_str!("../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        // Without revision, effective_version should equal stable version
        assert_eq!(formula.revision, 0);
        assert_eq!(formula.effective_version(), "1.2.3");
    }

    #[test]
    fn effective_version_with_revision() {
        // Manually construct formula with revision since we don't have a fixture for it yet
        let mut formula: Formula =
            serde_json::from_str(include_str!("../fixtures/formula_foo.json")).unwrap();
        formula.revision = 1;

        // With revision=1, effective_version should be "1.2.3_1"
        assert_eq!(formula.effective_version(), "1.2.3_1");
    }

    #[test]
    fn effective_version_ignores_rebuild_for_dir_name() {
        let fixture = include_str!("../fixtures/formula_with_rebuild.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        // With rebuild=1 but revision=0, effective_version should NOT have suffix
        assert_eq!(formula.bottle.stable.rebuild, 1);
        assert_eq!(formula.revision, 0);
        assert_eq!(formula.effective_version(), "8.0.1");
    }

    #[test]
    fn revision_field_defaults_to_zero() {
        // Formulas without revision field should default to 0
        let fixture = include_str!("../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();
        assert_eq!(formula.revision, 0);
    }
}
