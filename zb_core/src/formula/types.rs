use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum KegOnly {
    #[default]
    No,
    Yes,
    Reason(String),
}

impl<'de> Deserialize<'de> for KegOnly {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::Bool(true) => Ok(KegOnly::Yes),
            serde_json::Value::String(s) => Ok(KegOnly::Reason(s)),
            _ => Ok(KegOnly::No),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SourceUrl {
    pub url: String,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FormulaUrls {
    #[serde(default)]
    pub stable: Option<SourceUrl>,
    #[serde(default)]
    pub head: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RubySourceChecksum {
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsesFromMacos {
    Plain(String),
    WithContext { name: String, context: String },
}

impl<'de> Deserialize<'de> for UsesFromMacos {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(s) => Ok(UsesFromMacos::Plain(s)),
            serde_json::Value::Object(map) => {
                let (name, context) = map
                    .into_iter()
                    .next()
                    .ok_or_else(|| serde::de::Error::custom("empty uses_from_macos object"))?;
                let ctx = context.as_str().unwrap_or("runtime").to_string();
                Ok(UsesFromMacos::WithContext { name, context: ctx })
            }
            _ => Err(serde::de::Error::custom("unexpected uses_from_macos value")),
        }
    }
}

impl UsesFromMacos {
    pub fn name(&self) -> &str {
        match self {
            UsesFromMacos::Plain(name) => name,
            UsesFromMacos::WithContext { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Formula {
    pub name: String,
    pub versions: Versions,
    pub dependencies: Vec<String>,
    pub bottle: Bottle,
    #[serde(default)]
    pub revision: u32,
    #[serde(default)]
    pub keg_only: KegOnly,
    #[serde(default)]
    pub build_dependencies: Vec<String>,
    #[serde(default)]
    pub urls: Option<FormulaUrls>,
    #[serde(default)]
    pub ruby_source_path: Option<String>,
    #[serde(default)]
    pub ruby_source_checksum: Option<RubySourceChecksum>,
    #[serde(default)]
    pub uses_from_macos: Vec<UsesFromMacos>,
    #[serde(default)]
    pub requirements: Vec<serde_json::Value>,
    #[serde(default)]
    pub variations: Option<serde_json::Value>,
}

impl Formula {
    pub fn effective_version(&self) -> String {
        if self.revision > 0 {
            format!("{}_{}", self.versions.stable, self.revision)
        } else {
            self.versions.stable.clone()
        }
    }

    pub fn is_keg_only(&self) -> bool {
        self.name.contains('@') || !matches!(self.keg_only, KegOnly::No)
    }

    pub fn source_url(&self) -> Option<&SourceUrl> {
        self.urls.as_ref().and_then(|u| u.stable.as_ref())
    }

    pub fn has_source_url(&self) -> bool {
        self.source_url().is_some()
    }

    pub fn all_build_dependencies(&self) -> Vec<String> {
        let mut deps = self.build_dependencies.clone();
        for u in &self.uses_from_macos {
            #[cfg(not(target_os = "macos"))]
            deps.push(u.name().to_string());
        }
        deps
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
            include_str!("../../fixtures/formula_foo.json"),
            include_str!("../../fixtures/formula_bar.json"),
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
        let fixture = include_str!("../../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        // Without revision, effective_version should equal stable version
        assert_eq!(formula.revision, 0);
        assert_eq!(formula.effective_version(), "1.2.3");
    }

    #[test]
    fn effective_version_with_revision() {
        // Manually construct formula with revision since we don't have a fixture for it yet
        let mut formula: Formula =
            serde_json::from_str(include_str!("../../fixtures/formula_foo.json")).unwrap();
        formula.revision = 1;

        // With revision=1, effective_version should be "1.2.3_1"
        assert_eq!(formula.effective_version(), "1.2.3_1");
    }

    #[test]
    fn effective_version_ignores_rebuild_for_dir_name() {
        let fixture = include_str!("../../fixtures/formula_with_rebuild.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        // With rebuild=1 but revision=0, effective_version should NOT have suffix
        assert_eq!(formula.bottle.stable.rebuild, 1);
        assert_eq!(formula.revision, 0);
        assert_eq!(formula.effective_version(), "8.0.1");
    }

    #[test]
    fn revision_field_defaults_to_zero() {
        let fixture = include_str!("../../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();
        assert_eq!(formula.revision, 0);
    }

    #[test]
    fn keg_only_defaults_to_no() {
        let fixture = include_str!("../../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();
        assert_eq!(formula.keg_only, KegOnly::No);
        assert!(!formula.is_keg_only());
    }

    #[test]
    fn keg_only_deserializes_bool_true() {
        let json = r#"{
            "name": "libfoo",
            "versions": { "stable": "1.0" },
            "dependencies": [],
            "keg_only": true,
            "bottle": { "stable": { "files": {
                "arm64_sonoma": { "url": "https://x.com/a.tar.gz", "sha256": "aa" }
            }}}
        }"#;
        let formula: Formula = serde_json::from_str(json).unwrap();
        assert_eq!(formula.keg_only, KegOnly::Yes);
        assert!(formula.is_keg_only());
    }

    #[test]
    fn keg_only_deserializes_string_reason() {
        let json = r#"{
            "name": "libpq",
            "versions": { "stable": "16.0" },
            "dependencies": [],
            "keg_only": "it conflicts with PostgreSQL",
            "bottle": { "stable": { "files": {
                "arm64_sonoma": { "url": "https://x.com/a.tar.gz", "sha256": "aa" }
            }}}
        }"#;
        let formula: Formula = serde_json::from_str(json).unwrap();
        assert!(
            matches!(formula.keg_only, KegOnly::Reason(ref s) if s == "it conflicts with PostgreSQL")
        );
        assert!(formula.is_keg_only());
    }

    #[test]
    fn versioned_formula_is_keg_only() {
        let json = r#"{
            "name": "postgresql@15",
            "versions": { "stable": "15.8" },
            "dependencies": [],
            "bottle": { "stable": { "files": {
                "arm64_sonoma": { "url": "https://x.com/a.tar.gz", "sha256": "aa" }
            }}}
        }"#;
        let formula: Formula = serde_json::from_str(json).unwrap();
        assert_eq!(formula.keg_only, KegOnly::No);
        assert!(formula.is_keg_only());
    }
}
