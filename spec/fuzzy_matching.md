# Fuzzy Matching Specification

## Overview

The fuzzy matching system evaluates how well a directory name matches a user's search query. It combines character-level matching with contextual bonuses to rank results, favoring recently accessed directories and those with structured naming conventions.

## Input/Output

- **Input**: Directory name, search query (optional), last modification time
- **Output**: Numeric score, highlighted text with formatting tokens

## Algorithm Phases

### 1. Preprocessing

- Convert both directory name and query to lowercase for case-insensitive matching
- Check for date prefix pattern: `YYYY-MM-DD-` at start of directory name

### 2. Character Matching

Perform sequential matching of query characters against the directory name:

- Iterate through each character in the directory name
- For each query character found in sequence, record match position
- Track gaps between consecutive matches
- If entire query is not matched, score = 0 (entry filtered out)

### 3. Base Scoring

- **Character match**: +1.0 point per matched character
- **Word boundary bonus**: +1.0 if match occurs at word start (position 0 or after non-alphanumeric character)
- **Proximity bonus**: +2.0 / √(gap + 1) where gap is characters between consecutive matches
  - Consecutive matches (gap=0): +2.0
  - Gap of 1: +1.41
  - Gap of 5: +0.82

### 4. Score Multipliers

Applied **only to the fuzzy match score** (character matches + bonuses), not to contextual bonuses:

- **Density multiplier**: `fuzzy_score × (query_length / (last_match_position + 1))`
  - Rewards matches concentrated toward the beginning
- **Length penalty**: `fuzzy_score × (10 / (string_length + 10))`
  - Penalizes longer directory names

### 5. Contextual Bonuses

Added **after** multipliers are applied:

- **Date prefix bonus**: +2.0 if directory name starts with `YYYY-MM-DD-` pattern
- **Recency bonus**: +3.0 / √(hours_since_access + 1)
  - Just accessed: +3.0
  - 1 hour ago: +2.1
  - 24 hours ago: +0.6
  - 1 week ago: +0.2

### Final Score

```
final_score = (fuzzy_score × density × length) + date_bonus + recency_bonus
```

## Highlighting

Matched characters are wrapped with formatting tokens:
- `{b}` before matched character
- `{/b}` after matched character

## Scoring Examples

### Example 1: Perfect consecutive match (recent access)

- Directory: `2025-11-29-project`
- Query: `pro`
- Last accessed: 1 hour ago
- Matches: positions 11-12-13 (`p` `r` `o`)

**Score breakdown:**
- Fuzzy score:
  - Base: 3 × 1.0 = 3.0
  - Word boundary: +1.0 (at start of "project")
  - Proximity: +2.0 + 2.0 = 4.0 (consecutive)
  - Subtotal: 8.0
  - Density: × (3/14) ≈ ×0.214
  - Length: × (10/19) ≈ ×0.526
  - After multipliers: ≈ 0.90
- Contextual bonuses:
  - Date bonus: +2.0
  - Recency: +3.0/√2 ≈ +2.1
- **Final score: ≈ 5.0**

### Example 2: Scattered match (no date prefix, older)

- Directory: `my-old-project`
- Query: `pro`
- Last accessed: 24 hours ago
- Matches: positions 7-8-10 (`p` `r` `o`)

**Score breakdown:**
- Fuzzy score:
  - Base: 3 × 1.0 = 3.0
  - Word boundary: +1.0
  - Proximity: +2.0/√1 + 2.0/√2 ≈ 3.4
  - Subtotal: 7.4
  - Density: × (3/11) ≈ ×0.273
  - Length: × (10/24) ≈ ×0.417
  - After multipliers: ≈ 0.84
- Contextual bonuses:
  - Date bonus: +0.0
  - Recency: +3.0/√25 = +0.6
- **Final score: ≈ 1.4**

## Design Principles

- **Favor recency**: Recently accessed directories appear higher
- **Structured naming**: Date-prefixed directories get priority
- **Word boundaries**: Matches at logical breaks score higher
- **Consecutive matches**: Characters close together score better
- **Early matches**: Matches near string start are preferred
- **Conciseness**: Shorter directory names are favored

## Filtering Behavior

- Entries with score = 0 are hidden from results
- Zero score occurs when query characters cannot be matched in sequence
- Partial matches are not allowed - all query characters must be found

## Pseudo-code

```rust
fn process_entries(query: &str, entries: &[DirEntry]) -> Vec<(usize, f64, String)> {
    let mut results = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let has_date_prefix = is_date_prefix(&entry.name);
        let date_bonus = if has_date_prefix { 2.0 } else { 0.0 };

        // No query - score by recency only
        if query.is_empty() {
            let tokenized = if has_date_prefix {
                format!("{{dim}}{}{{/fg}}{}", &entry.name[..11], &entry.name[11..])
            } else {
                entry.name.clone()
            };
            let score = date_bonus + recency_bonus(entry.mtime);
            results.push((idx, score, tokenized));
        } else {
            // Fuzzy matching
            if let Some(result) = fuzzy_match(&entry.name, query) {
                let mut fuzzy_score = result.score;

                // Apply multipliers to fuzzy score only
                fuzzy_score *= query.len() as f64 / (result.last_match_pos + 1) as f64;
                fuzzy_score *= 10.0 / (entry.name.len() as f64 + 10.0);

                // Add contextual bonuses after multipliers
                let final_score = fuzzy_score + date_bonus + recency_bonus(entry.mtime);
                results.push((idx, final_score, result.highlighted));
            }
        }
    }
    results
}

fn fuzzy_match(text: &str, query: &str) -> Option<MatchResult> {
    let mut score = 0.0;
    let mut last_match_pos: i64 = -1;
    let mut highlighted = String::new();
    let mut query_idx = 0;

    for (pos, char) in text.chars().enumerate() {
        if query_idx < query.len() && char.to_lowercase().eq(query.chars().nth(query_idx).unwrap().to_lowercase()) {
            score += 1.0;  // Base match

            // Word boundary bonus
            if pos == 0 || !text.as_bytes()[pos - 1].is_ascii_alphanumeric() {
                score += 1.0;
            }

            // Proximity bonus
            if last_match_pos >= 0 {
                let gap = pos as i64 - last_match_pos - 1;
                score += 2.0 / ((gap + 1) as f64).sqrt();
            }

            last_match_pos = pos as i64;
            query_idx += 1;
            highlighted.push_str(&format!("{{b}}{}{{/b}}", char));
        } else {
            highlighted.push(char);
        }
    }

    if query_idx < query.len() { return None; }  // Incomplete match

    Some(MatchResult { score, last_match_pos: last_match_pos as usize, highlighted })
}

fn recency_bonus(mtime: f64) -> f64 {
    let now = current_time_secs();
    let hours = (now - mtime) / 3600.0;
    3.0 / (hours + 1.0).sqrt()
}
```
