pub mod bottle;
pub mod resolve;
pub mod types;

pub use bottle::{SelectedBottle, select_bottle};
pub use resolve::resolve_closure;
pub use types::{
    Bottle, BottleFile, BottleStable, Formula, FormulaUrls, KegOnly, KegOnlyReason,
    RubySourceChecksum, SourceUrl, UsesFromMacos, Versions,
};

/// Extract the formula token from an install key.
/// Examples:
/// - `wget` -> `wget`
/// - `hashicorp/tap/terraform` -> `terraform`
pub fn formula_token(name: &str) -> &str {
    if name.is_empty() {
        return "";
    }

    name.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::formula_token;

    #[test]
    fn formula_token_keeps_core_formula_name() {
        assert_eq!(formula_token("wget"), "wget");
    }

    #[test]
    fn formula_token_extracts_tap_formula_name() {
        assert_eq!(formula_token("hashicorp/tap/terraform"), "terraform");
    }

    #[test]
    fn formula_token_handles_empty_name_explicitly() {
        assert_eq!(formula_token(""), "");
    }

    #[test]
    fn formula_token_ignores_trailing_separator() {
        assert_eq!(formula_token("hashicorp/tap/terraform/"), "terraform");
    }

    #[test]
    fn formula_token_handles_only_separators() {
        assert_eq!(formula_token("///"), "");
    }
}
