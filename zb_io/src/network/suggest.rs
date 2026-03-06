use strsim::damerau_levenshtein;

const MIN_SIMILARITY_SCORE: f64 = 0.45;

#[derive(Debug, Clone, PartialEq)]
struct CandidateScore {
    name: String,
    score: f64,
    distance: usize,
    len_delta: usize,
}

pub fn rank_formula_suggestions(query: &str, candidates: &[String], limit: usize) -> Vec<String> {
    rank_formula_suggestions_with(query, candidates, limit, damerau_levenshtein)
}

fn rank_formula_suggestions_with<F>(
    query: &str,
    candidates: &[String],
    limit: usize,
    mut distance_fn: F,
) -> Vec<String>
where
    F: FnMut(&str, &str) -> usize,
{
    if limit == 0 {
        return Vec::new();
    }

    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<CandidateScore> = candidates
        .iter()
        .filter_map(|candidate| {
            let normalized = candidate.trim().to_ascii_lowercase();
            if normalized.is_empty() || !is_plausible_candidate(&query, &normalized) {
                return None;
            }

            let distance = distance_fn(&query, &normalized);
            let max_len = query.len().max(normalized.len());
            let similarity = if max_len == 0 {
                1.0
            } else {
                1.0 - (distance as f64 / max_len as f64)
            };

            let score = similarity;
            if score < MIN_SIMILARITY_SCORE {
                return None;
            }

            Some(CandidateScore {
                name: candidate.clone(),
                score,
                distance,
                len_delta: query.len().abs_diff(normalized.len()),
            })
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.distance.cmp(&b.distance))
            .then_with(|| a.len_delta.cmp(&b.len_delta))
            .then_with(|| a.name.cmp(&b.name))
    });

    scored.into_iter().take(limit).map(|s| s.name).collect()
}

fn is_plausible_candidate(query: &str, candidate: &str) -> bool {
    query.len().abs_diff(candidate.len()) <= max_len_delta(query.len())
}

fn max_len_delta(query_len: usize) -> usize {
    query_len.saturating_mul(2) / 3 + 1
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use strsim::damerau_levenshtein;

    use super::{max_len_delta, rank_formula_suggestions, rank_formula_suggestions_with};

    #[test]
    fn ranks_common_typo_as_top_match() {
        let candidates = vec![
            "python".to_string(),
            "pytest".to_string(),
            "pypy".to_string(),
        ];

        let suggestions = rank_formula_suggestions("pythn", &candidates, 3);
        assert_eq!(suggestions.first().map(String::as_str), Some("python"));
    }

    #[test]
    fn filters_unrelated_candidates() {
        let candidates = vec![
            "wget".to_string(),
            "ripgrep".to_string(),
            "zstd".to_string(),
        ];

        let suggestions = rank_formula_suggestions("completelydifferent", &candidates, 3);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn respects_result_limit() {
        let candidates = vec![
            "git".to_string(),
            "gitea".to_string(),
            "git-lfs".to_string(),
            "glow".to_string(),
        ];

        let suggestions = rank_formula_suggestions("git", &candidates, 2);
        assert_eq!(suggestions.len(), 2);
    }

    #[test]
    fn skips_edit_distance_for_implausible_length_deltas() {
        let candidates = vec![
            "git".to_string(),
            "gitea".to_string(),
            "super-long-package-name".to_string(),
            "another-extremely-long-formula-name".to_string(),
        ];
        let distance_calls = Cell::new(0usize);

        let suggestions =
            rank_formula_suggestions_with("git", &candidates, 3, |query, candidate| {
                distance_calls.set(distance_calls.get() + 1);
                damerau_levenshtein(query, candidate)
            });

        assert_eq!(distance_calls.get(), 2);
        assert_eq!(suggestions.first().map(String::as_str), Some("git"));
    }

    #[test]
    fn max_len_delta_scales_with_query_length() {
        assert_eq!(max_len_delta(3), 3);
        assert_eq!(max_len_delta(5), 4);
        assert_eq!(max_len_delta(9), 7);
    }

    #[test]
    fn does_not_keep_short_prefix_only_matches() {
        let candidates = vec![
            "git".to_string(),
            "gnupg".to_string(),
            "ripgrep".to_string(),
        ];

        let suggestions = rank_formula_suggestions("gitignore", &candidates, 3);

        assert!(!suggestions.iter().any(|candidate| candidate == "git"));
    }
}
