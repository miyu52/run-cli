//! Spelling suggestions for unresolvable tool names.

use std::path::Path;

use strsim::levenshtein;

/// Maximum edit distance for which a suggestion is offered.
const MAX_DISTANCE: usize = 2;

/// Find the closest existing tool for a misspelled name.
///
/// Candidates are compared both by full file name and by name without its
/// extension, so a bare query like `echo` can suggest `echo.bat`. Returns the
/// closest candidate within [`MAX_DISTANCE`], if any.
pub fn suggest(query: &str, candidates: &[String]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for name in candidates {
        for candidate in [name.as_str(), base_name(name)] {
            let distance = levenshtein(query, candidate);
            if distance <= MAX_DISTANCE
                && best.is_none_or(|(best_distance, _)| distance < best_distance)
            {
                best = Some((distance, name.as_str()));
            }
        }
    }
    best.map(|(_, name)| name.to_string())
}

fn base_name(name: &str) -> &str {
    Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn suggests_close_name() {
        assert_eq!(
            suggest("echoe", &names(&["echo.bat", "exit42.bat"])),
            Some("echo.bat".to_string())
        );
    }

    #[test]
    fn suggests_matching_base_name() {
        assert_eq!(
            suggest("echo", &names(&["echo.bat"])),
            Some("echo.bat".to_string())
        );
    }

    #[test]
    fn no_suggestion_for_distant_names() {
        assert_eq!(suggest("zzz", &names(&["echo.bat"])), None);
    }

    #[test]
    fn prefers_exact_base_match_over_closer_full_name() {
        assert_eq!(
            suggest("ex42", &names(&["exit42.bat", "ex42.exe"])),
            Some("ex42.exe".to_string())
        );
    }

    #[test]
    fn empty_candidates() {
        assert_eq!(suggest("echo", &[]), None);
    }
}
