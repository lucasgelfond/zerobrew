use crate::{Error, Formula};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedBottle {
    pub tag: String,
    pub url: String,
    pub sha256: String,
}

pub fn select_bottle(formula: &Formula) -> Result<SelectedBottle, Error> {
    for (tag, file) in &formula.bottle.stable.files {
        if tag.starts_with("arm64_") {
            return Ok(SelectedBottle {
                tag: tag.clone(),
                url: file.url.clone(),
                sha256: file.sha256.clone(),
            });
        }
    }

    Err(Error::UnsupportedBottle {
        name: formula.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{Bottle, BottleFile, BottleStable, Versions};
    use std::collections::BTreeMap;

    #[test]
    fn selects_arm64_bottle() {
        let fixture = include_str!("../fixtures/formula_foo.json");
        let formula: Formula = serde_json::from_str(fixture).unwrap();

        let selected = select_bottle(&formula).unwrap();
        assert_eq!(selected.tag, "arm64_sonoma");
        assert_eq!(
            selected.url,
            "https://example.com/foo-1.2.3.arm64_sonoma.bottle.tar.gz"
        );
        assert_eq!(
            selected.sha256,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn errors_when_no_arm64_bottle() {
        let mut files = BTreeMap::new();
        files.insert(
            "x86_64_sonoma".to_string(),
            BottleFile {
                url: "https://example.com/legacy.tar.gz".to_string(),
                sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    .to_string(),
            },
        );

        let formula = Formula {
            name: "legacy".to_string(),
            versions: Versions {
                stable: "0.1.0".to_string(),
            },
            dependencies: Vec::new(),
            bottle: Bottle {
                stable: BottleStable { files },
            },
        };

        let err = select_bottle(&formula).unwrap_err();
        assert!(matches!(
            err,
            Error::UnsupportedBottle { name } if name == "legacy"
        ));
    }
}
