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
            if normalized.is_empty() {
                return None;
            }

            let distance = damerau_levenshtein(&query, &normalized);
            let max_len = query.len().max(normalized.len());
            let similarity = if max_len == 0 {
                1.0
            } else {
                1.0 - (distance as f64 / max_len as f64)
            };

            let prefix_bonus = if normalized.starts_with(&query) || query.starts_with(&normalized) {
                0.15
            } else {
                0.0
            };

            let score = (similarity + prefix_bonus).min(1.0);
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

#[cfg(test)]
mod tests {
    use super::rank_formula_suggestions;

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
}
