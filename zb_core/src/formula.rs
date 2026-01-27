use serde::Deserialize;
use std::collections::BTreeMap;

use crate::Error;
use crate::validation::{validate_dependency_name, validate_formula_name, validate_version};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Formula {
    pub name: String,
    pub versions: Versions,
    pub dependencies: Vec<String>,
    pub bottle: Bottle,
}

impl Formula {
    /// Validate formula after deserialization
    pub fn validate(&self) -> Result<(), Error> {
        // Validate formula name
        validate_formula_name(&self.name)?;

        // Validate version
        validate_version(&self.versions.stable)?;

        // Validate all dependencies
        for dep in &self.dependencies {
            validate_dependency_name(dep)?;
        }

        Ok(())
    }

    /// Returns the effective version including rebuild suffix if applicable.
    /// Homebrew bottles with rebuild > 0 have paths like `{version}_{rebuild}`.
    pub fn effective_version(&self) -> String {
        let rebuild = self.bottle.stable.rebuild;
        if rebuild > 0 {
            format!("{}_{}", self.versions.stable, rebuild)
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
    fn effective_version_without_rebuild() {
        let fixture = include_str!("../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        // Without rebuild, effective_version should equal stable version
        assert_eq!(formula.bottle.stable.rebuild, 0);
        assert_eq!(formula.effective_version(), "1.2.3");
    }

    #[test]
    fn effective_version_with_rebuild() {
        let fixture = include_str!("../fixtures/formula_with_rebuild.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        // With rebuild=1, effective_version should be "8.0.1_1"
        assert_eq!(formula.bottle.stable.rebuild, 1);
        assert_eq!(formula.effective_version(), "8.0.1_1");
    }

    #[test]
    fn rebuild_field_defaults_to_zero() {
        // Formulas without rebuild field should default to 0
        let fixture = include_str!("../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();
        assert_eq!(formula.bottle.stable.rebuild, 0);
    }
}
