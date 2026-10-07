pub fn matches(query: &str, candidate: &str) -> bool {
    let candidate = candidate.to_lowercase();
    query
        .split_whitespace()
        .all(|token| candidate.contains(&token.to_lowercase()))
}

pub fn filtered<'a, T>(query: &str, items: &'a [T], label: impl Fn(&T) -> &str) -> Vec<&'a T> {
    items
        .iter()
        .filter(|item| matches(query, label(item)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_is_token_substring_case_insensitive() {
        assert!(matches("wo op", "Open Work…"));
        assert!(!matches("open zip", "Open Work…"));
    }
}
