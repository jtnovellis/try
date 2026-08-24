//! Fuzzy string matching with scoring and highlight positions.
//! Direct port of lib/fuzzy.rb.

/// Raw directory entry data loaded from the tries path.
#[derive(Clone)]
pub struct DirEntry {
    pub text: String,
    pub text_lower: String,
    pub base_score: f64,
    pub path: String,
    pub is_symlink: bool,
    pub mtime_secs: f64,
}

#[derive(Clone)]
pub struct MatchResult {
    entry: DirEntry,
    score: f64,
    positions: Vec<usize>,
}

impl MatchResult {
    pub fn from(m: &MatchResult) -> MatchResult {
        m.clone()
    }
}

fn sqrt_table(gap: usize) -> f64 {
    // 2.0 / sqrt(gap + 1)
    // Precompute at compile time would be ideal; compute at first use.
    // For simplicity, compute inline (the table in Ruby is a frozen array).
    2.0 / ((gap as f64 + 1.0).sqrt())
}

/// Calculate the fuzzy match score for an entry against a query.
/// Returns (score, positions) or None if no match.
pub fn calculate_match(entry: &DirEntry, query: &str, query_lower: &str, query_chars: &[u8]) -> Option<(f64, Vec<usize>)> {
    let mut positions = Vec::new();
    let mut score = entry.base_score;

    // Empty query = match all with base score only
    if query.is_empty() {
        return Some((score, positions));
    }

    let text = entry.text_lower.as_bytes();
    let mut last_pos: i64 = -1;
    let mut pos = 0usize;

    for &qc in query_chars {
        // Find next occurrence of query char starting from pos
        let mut found = None;
        let mut i = pos;
        while i < text.len() {
            if text[i] == qc {
                found = Some(i);
                break;
            }
            i += 1;
        }
        let found = found?;

        positions.push(found);

        // Base match point
        score += 1.0;

        // Word boundary bonus (start of string or after non-alphanumeric)
        let boundary = found == 0 || {
            let prev = text[found - 1];
            !(prev.is_ascii_alphanumeric())
        };
        if boundary {
            score += 1.0;
        }

        // Proximity bonus (consecutive chars score higher)
        if last_pos >= 0 {
            let gap = (found as i64) - last_pos - 1;
            if (0..64).contains(&gap) {
                score += sqrt_table(gap as usize);
            } else if gap >= 0 {
                score += 2.0 / ((gap as f64 + 1.0).sqrt());
            }
        }

        last_pos = found as i64;
        pos = found + 1;
    }

    // Density bonus: prefer shorter spans
    let last_match = last_pos as usize;
    score *= (query_lower.len() as f64) / (last_match + 1) as f64;

    // Length penalty: shorter strings score higher
    score *= 10.0 / (entry.text.len() as f64 + 10.0);

    Some((score, positions))
}

/// Match all entries against a query, returning sorted + limited results.
pub fn fuzzy_match(entries: &[DirEntry], query: &str) -> Vec<MatchResult> {
    let query_lower = query.to_lowercase();
    let query_chars: Vec<u8> = query_lower.as_bytes().to_vec();

    let mut results = Vec::new();
    for entry in entries {
        if let Some((score, positions)) = calculate_match(entry, query, &query_lower, &query_chars) {
            results.push(MatchResult {
                entry: entry.clone(),
                score,
                positions,
            });
        }
    }

    // Sort by score descending (Spinel has no Array#max_by; full sort is fine at scale)
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    results
}

impl MatchResult {
    pub fn entry(&self) -> &DirEntry {
        &self.entry
    }
    pub fn score(&self) -> f64 {
        self.score
    }
    pub fn positions(&self) -> &[usize] {
        &self.positions
    }
}
