//! Fuzzy ranking of workspace names against a query.

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub index: usize,
    pub score: i64,
    pub positions: Vec<usize>,
}

pub fn rank(candidates: &[String], query: &str) -> Vec<Match> {
    if query.is_empty() {
        return candidates
            .iter()
            .enumerate()
            .map(|(index, _)| Match {
                index,
                score: 0,
                positions: Vec::new(),
            })
            .collect();
    }
    let matcher = SkimMatcherV2::default();
    let mut matches: Vec<Match> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, cand)| {
            matcher
                .fuzzy_indices(cand, query)
                .map(|(score, positions)| Match {
                    index,
                    score,
                    positions,
                })
        })
        .collect();
    // Descending score, ties broken by ascending index for stable display.
    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["auth", "api", "docs", "default"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn empty_query_returns_all_in_order() {
        let m = rank(&names(), "");
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].index, 0);
        assert_eq!(m[3].index, 3);
    }

    #[test]
    fn filters_and_ranks_matches() {
        let m = rank(&names(), "au");
        // "auth" and "default" both contain a/u subsequence; "auth" should rank first.
        assert!(!m.is_empty());
        assert_eq!(names()[m[0].index], "auth");
        assert!(m.iter().all(|x| names()[x.index] != "docs"));
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(rank(&names(), "zzzz").is_empty());
    }

    #[test]
    fn reports_match_positions() {
        let m = rank(&names(), "ath"); // a..t..h in "auth"
        let top = &m[0];
        assert_eq!(names()[top.index], "auth");
        assert!(!top.positions.is_empty());
    }
}
