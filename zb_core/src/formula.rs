use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Formula {
    pub name: String,
    pub versions: Versions,
    pub dependencies: Vec<String>,
    pub bottle: Bottle,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Versions {
    pub stable: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct Bottle {
    pub stable: BottleStable,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct BottleStable {
    pub files: BTreeMap<String, BottleFile>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
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
}
