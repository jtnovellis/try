use std::path::Path;

/// Parsed git URI components
pub struct GitUri {
    pub user: String,
    pub repo: String,
}

pub struct GithubPr {
    pub user: String,
    pub repo: String,
    pub pr_id: String,
    pub git_uri: String,
}

/// Strip a trailing `.git` suffix from a URI string.
fn strip_git_suffix(uri: &str) -> &str {
    uri.strip_suffix(".git").unwrap_or(uri)
}

/// Parse a git URI into user/repo components.
/// Matches the Ruby `parse_git_uri` ordering exactly.
pub fn parse_git_uri(raw_uri: &str) -> Option<GitUri> {
    let uri = strip_git_suffix(raw_uri);

    // https://github.com/user/repo
    if let Some(caps) = simple_regex_capture(uri, r"^https?://github\.com/([^/]+)/([^/]+)") {
        return Some(GitUri {
            user: caps[0].clone(),
            repo: caps[1].clone(),
        });
    }

    // git@github.com:user/repo
    if let Some(caps) = simple_regex_capture(uri, r"^git@github\.com:([^/]+)/([^/]+)") {
        return Some(GitUri {
            user: caps[0].clone(),
            repo: caps[1].clone(),
        });
    }

    // https://gitlab.com/user/repo or other git hosts
    if let Some(caps) = simple_regex_capture(uri, r"^https?://([^/]+)/([^/]+)/([^/]+)") {
        return Some(GitUri {
            user: caps[1].clone(),
            repo: caps[2].clone(),
        });
    }

    // git@host:user/path/to/repo
    if let Some(caps) = simple_regex_capture(uri, r"^git@([^:]+):([^/]+)/(.+)") {
        let path = &caps[2];
        let repo = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        return Some(GitUri {
            user: caps[1].clone(),
            repo,
        });
    }

    // ssh://user@host:port/user/repo  →  ssh://[^@/]+@([^/]+)/([^/]+)/(.+)
    if let Some(caps) = simple_regex_capture(uri, r"^ssh://[^@/]+@([^/]+)/([^/]+)/(.+)") {
        let path = &caps[2];
        let repo = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        return Some(GitUri {
            user: caps[1].clone(),
            repo,
        });
    }

    // SCP-style SSH: user@host:path/to/repo  →  ([^@/:]+)@([^:]+):(.+)
    if let Some(caps) = simple_regex_capture(uri, r"^([^@/:]+)@([^:]+):(.+)") {
        let path = &caps[2];
        let repo = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        return Some(GitUri {
            user: caps[0].clone(),
            repo,
        });
    }

    None
}

/// Parse a GitHub PR URL into clone details.
/// https://github.com/user/repo/pull/123
pub fn github_pr_details(uri: &str) -> Option<GithubPr> {
    let caps = simple_regex_capture(
        uri,
        r"^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/pull/(\d+)/?$",
    )?;

    let user = caps[0].clone();
    let repo = strip_git_suffix(&caps[1]).to_string();
    let pr_id = caps[2].clone();
    let git_uri = format!("https://github.com/{}/{}.git", user, repo);

    Some(GithubPr {
        user,
        repo,
        pr_id,
        git_uri,
    })
}

/// Determine if an argument looks like a git URI.
pub fn is_git_uri(arg: &str) -> bool {
    arg.starts_with("https://")
        || arg.starts_with("http://")
        || arg.starts_with("git@")
        || arg.contains("github.com")
        || arg.contains("gitlab.com")
        || arg.ends_with(".git")
}

/// Generate the directory name for a clone: YYYY-MM-DD-user-repo, or custom.
pub fn generate_clone_directory_name(git_uri: &str, custom_name: Option<&str>) -> Option<String> {
    if let Some(name) = custom_name {
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    // Try PR first, then regular git URI. Both expose user/repo for the name.
    let (user, repo) = if let Some(pr) = github_pr_details(git_uri) {
        (pr.user, pr.repo)
    } else {
        let g = parse_git_uri(git_uri)?;
        (g.user, g.repo)
    };

    let date_prefix = crate::date::today_date_prefix();
    Some(format!("{}-{}-{}", date_prefix, user, repo))
}

// ---------------------------------------------------------------------------
// Minimal regex engine — just enough for the fixed patterns above.
// Supports: literals, ^, $, ., [^...], [chars], (...), +, *, {n}, {n,}, {n,m}, \., \d, \A, \z, ?:
// Returns capture groups as String vectors.
// ---------------------------------------------------------------------------

/// Capture groups (1-indexed groups are returned in order).
pub fn simple_regex_capture(text: &str, pattern: &str) -> Option<Vec<String>> {
    let re = Regex::new(pattern)?;
    re.captures(text)
}

struct Regex {
    nodes: Vec<Node>,
    anchored_start: bool,
    anchored_end: bool,
}

enum Node {
    Literal(u8),
    Dot,
    // A character class: list of allowed bytes + ranges.
    Class { ranges: Vec<(u8, u8)>, negate: bool },
    // A capture group — contains a sequence of sub-nodes.
    Group(Vec<Node>),
    // Repetition of a single node
    Repeat(Box<Node>, RepKind),
    // \d
    Digit,
    // Non-capturing group
    NonCap(Vec<Node>),
    // Optional non-capturing prefix (?:...) — treated as NonCap
}

enum RepKind {
    Plus,    // one or more
    Star,    // zero or more
    Question, // zero or one
}

impl Regex {
    fn new(pattern: &str) -> Option<Regex> {
        let bytes = pattern.as_bytes();
        let mut anchored_start = false;
        let mut anchored_end = false;
        let mut i = 0;

        // Strip leading \A or ^
        if bytes.starts_with(b"\\A") {
            anchored_start = true;
            i = 2;
        } else if bytes.first() == Some(&b'^') {
            anchored_start = true;
            i = 1;
        }

        // Strip trailing \z or $
        let mut end = bytes.len();
        if bytes.ends_with(b"\\z") {
            anchored_end = true;
            end -= 2;
        } else if end > 0 && bytes[end - 1] == b'$' {
            anchored_end = true;
            end -= 1;
        }

        let (nodes, _) = parse_nodes(bytes, i, end)?;
        Some(Regex {
            nodes,
            anchored_start,
            anchored_end,
        })
    }

    fn captures(&self, text: &str) -> Option<Vec<String>> {
        let bytes = text.as_bytes();
        let start_positions: Vec<usize> = if self.anchored_start {
            vec![0]
        } else {
            (0..=bytes.len()).collect()
        };

        for start in start_positions {
            let mut groups = Vec::new();
            if let Some(end_pos) = self.match_nodes(&self.nodes, bytes, start, &mut groups) {
                if self.anchored_end && end_pos != bytes.len() {
                    continue;
                }
                return Some(groups);
            }
        }
        None
    }

    /// Try to match `nodes` at position `pos`. Returns the end position on success.
    fn match_nodes(
        &self,
        nodes: &[Node],
        bytes: &[u8],
        pos: usize,
        groups: &mut Vec<String>,
    ) -> Option<usize> {
        self.match_seq(nodes, 0, bytes, pos, groups)
    }

    /// Match nodes[idx..] starting at pos, backtracking on repeats.
    fn match_seq(
        &self,
        nodes: &[Node],
        idx: usize,
        bytes: &[u8],
        pos: usize,
        groups: &mut Vec<String>,
    ) -> Option<usize> {
        if idx >= nodes.len() {
            return Some(pos);
        }

        match &nodes[idx] {
            Node::Literal(b) => {
                if pos < bytes.len() && bytes[pos] == *b {
                    self.match_seq(nodes, idx + 1, bytes, pos + 1, groups)
                } else {
                    None
                }
            }
            Node::Dot => {
                if pos < bytes.len() {
                    self.match_seq(nodes, idx + 1, bytes, pos + 1, groups)
                } else {
                    None
                }
            }
            Node::Digit => {
                if pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    self.match_seq(nodes, idx + 1, bytes, pos + 1, groups)
                } else {
                    None
                }
            }
            Node::Class { ranges, negate } => {
                if pos < bytes.len() {
                    let c = bytes[pos];
                    let matched = ranges.iter().any(|(lo, hi)| c >= *lo && c <= *hi);
                    if matched != *negate {
                        return self.match_seq(nodes, idx + 1, bytes, pos + 1, groups);
                    }
                }
                None
            }
            Node::Group(inner) => {
                // Capture: try to match inner, record the matched substring.
                let group_start = pos;
                let groups_len = groups.len();
                // We need to try matching inner greedily, then continuing.
                if let Some(end) = self.match_group_greedy(inner, bytes, group_start, groups) {
                    // On success, record the group, then continue
                    let captured = String::from_utf8_lossy(&bytes[group_start..end]).to_string();
                    // groups may have been modified by inner groups; insert after them
                    // We store this group at the right position by truncating inner subgroups
                    // and adding ours. Actually for our patterns, groups don't nest captures,
                    // so inner groups (if any) are NonCap. We'll record ours.
                    // But inner subgroups were not captured (they're NonCap or non-group nodes).
                    groups.truncate(groups_len);
                    groups.push(captured);
                    if let Some(final_end) = self.match_seq(nodes, idx + 1, bytes, end, groups) {
                        return Some(final_end);
                    }
                    groups.truncate(groups_len);
                }
                None
            }
            Node::NonCap(inner) => {
                if let Some(end) = self.match_group_greedy(inner, bytes, pos, groups) {
                    self.match_seq(nodes, idx + 1, bytes, end, groups)
                } else {
                    None
                }
            }
            Node::Repeat(inner, kind) => {
                self.match_repeat(inner, kind, nodes, idx, bytes, pos, groups)
            }
        }
    }

    /// Greedily match a group's inner nodes, backtracking as needed so the
    /// overall sequence can continue. We try the longest match first.
    fn match_group_greedy(
        &self,
        inner: &[Node],
        bytes: &[u8],
        pos: usize,
        groups: &mut Vec<String>,
    ) -> Option<usize> {
        self.match_seq(inner, 0, bytes, pos, groups)
    }

    #[allow(clippy::too_many_arguments)]
    fn match_repeat(
        &self,
        inner: &Node,
        kind: &RepKind,
        nodes: &[Node],
        idx: usize,
        bytes: &[u8],
        pos: usize,
        groups: &mut Vec<String>,
    ) -> Option<usize> {
        match kind {
            RepKind::Plus => {
                // One or more: must match at least once
                self.match_repeat_n(inner, 1, None, nodes, idx, bytes, pos, groups)
            }
            RepKind::Star => {
                // Zero or more
                self.match_repeat_n(inner, 0, None, nodes, idx, bytes, pos, groups)
            }
            RepKind::Question => {
                // Zero or one: try one first (greedy)
                let groups_len = groups.len();
                if let Some(end) = self.match_single(inner, bytes, pos, groups) {
                    if let Some(final_end) = self.match_seq(nodes, idx + 1, bytes, end, groups) {
                        return Some(final_end);
                    }
                    groups.truncate(groups_len);
                }
                // Try zero
                self.match_seq(nodes, idx + 1, bytes, pos, groups)
            }
        }
    }

    /// Match `min` or more repetitions (up to `max` if specified) of `inner`.
    /// Tries greedily (longest first).
    #[allow(clippy::too_many_arguments)]
    fn match_repeat_n(
        &self,
        inner: &Node,
        min: usize,
        max: Option<usize>,
        nodes: &[Node],
        idx: usize,
        bytes: &[u8],
        pos: usize,
        groups: &mut Vec<String>,
    ) -> Option<usize> {
        // Collect all possible match positions through greedy backtracking.
        // We recursively try: match as many as possible, then backtrack.
        self.repeat_backtrack(inner, min, max, 0, nodes, idx, bytes, pos, groups)
    }

    #[allow(clippy::too_many_arguments)]
    fn repeat_backtrack(
        &self,
        inner: &Node,
        min: usize,
        max: Option<usize>,
        count: usize,
        nodes: &[Node],
        idx: usize,
        bytes: &[u8],
        pos: usize,
        groups: &mut Vec<String>,
    ) -> Option<usize> {
        // Try matching one more (greedy), unless we've hit max
        let can_match_more = max.is_none_or(|m| count < m);
        if can_match_more {
            let groups_len = groups.len();
            if let Some(end) = self.match_single(inner, bytes, pos, groups) {
                if end > pos {
                    if let Some(final_end) =
                        self.repeat_backtrack(inner, min, max, count + 1, nodes, idx, bytes, end, groups)
                    {
                        return Some(final_end);
                    }
                }
                groups.truncate(groups_len);
            }
        }
        // If we've met the minimum, try stopping here
        if count >= min {
            return self.match_seq(nodes, idx + 1, bytes, pos, groups);
        }
        None
    }

    /// Match a single instance of `inner` at `pos`.
    fn match_single(
        &self,
        inner: &Node,
        bytes: &[u8],
        pos: usize,
        groups: &mut Vec<String>,
    ) -> Option<usize> {
        match inner {
            Node::Literal(b) => {
                if pos < bytes.len() && bytes[pos] == *b {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            Node::Dot => {
                if pos < bytes.len() {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            Node::Digit => {
                if pos < bytes.len() && bytes[pos].is_ascii_digit() {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            Node::Class { ranges, negate } => {
                if pos < bytes.len() {
                    let c = bytes[pos];
                    let matched = ranges.iter().any(|(lo, hi)| c >= *lo && c <= *hi);
                    if matched != *negate {
                        return Some(pos + 1);
                    }
                }
                None
            }
            Node::Group(inner_nodes) => {
                let group_start = pos;
                let groups_len = groups.len();
                if let Some(end) = self.match_seq(inner_nodes, 0, bytes, pos, groups) {
                    let captured = String::from_utf8_lossy(&bytes[group_start..end]).to_string();
                    groups.truncate(groups_len);
                    groups.push(captured);
                    Some(end)
                } else {
                    None
                }
            }
            Node::NonCap(inner_nodes) => self.match_seq(inner_nodes, 0, bytes, pos, groups),
            Node::Repeat(inner2, kind2) => {
                self.match_repeat(inner2, kind2, &[], 0, bytes, pos, groups)
            }
        }
    }
}

/// Parse nodes from bytes[start..end). Returns (nodes, new_pos).
fn parse_nodes(bytes: &[u8], start: usize, end: usize) -> Option<(Vec<Node>, usize)> {
    let mut nodes = Vec::new();
    let mut i = start;
    while i < end {
        let (node, next) = parse_node(bytes, i, end)?;
        if let Some(n) = node { nodes.push(n) }
        i = next;
    }
    Some((nodes, i))
}

/// Parse a single node (possibly with repetition suffix).
fn parse_node(bytes: &[u8], i: usize, end: usize) -> Option<(Option<Node>, usize)> {
    if i >= end {
        return Some((None, i));
    }
    let b = bytes[i];

    // (?:...) non-capturing group
    if b == b'(' && i + 2 < end && bytes[i + 1] == b'?' && bytes[i + 2] == b':' {
        let group_start = i + 3;
        let (inner, after) = parse_group_inner(bytes, group_start, end)?;
        // Check for repetition after )
        let mut pos = after;
        if pos < end && (bytes[pos] == b'+' || bytes[pos] == b'*' || bytes[pos] == b'?') {
            let kind = match bytes[pos] {
                b'+' => RepKind::Plus,
                b'*' => RepKind::Star,
                b'?' => RepKind::Question,
                _ => unreachable!(),
            };
            pos += 1;
            return Some((
                Some(Node::Repeat(Box::new(Node::NonCap(inner)), kind)),
                pos,
            ));
        }
        return Some((Some(Node::NonCap(inner)), after));
    }

    // (...) capturing group
    if b == b'(' {
        let group_start = i + 1;
        let (inner, after) = parse_group_inner(bytes, group_start, end)?;
        let mut pos = after;
        if pos < end && (bytes[pos] == b'+' || bytes[pos] == b'*' || bytes[pos] == b'?') {
            let kind = match bytes[pos] {
                b'+' => RepKind::Plus,
                b'*' => RepKind::Star,
                b'?' => RepKind::Question,
                _ => unreachable!(),
            };
            pos += 1;
            return Some((
                Some(Node::Repeat(Box::new(Node::Group(inner)), kind)),
                pos,
            ));
        }
        return Some((Some(Node::Group(inner)), after));
    }

    // [..] character class
    if b == b'[' {
        return parse_char_class(bytes, i, end);
    }

    // . dot
    if b == b'.' {
        let mut pos = i + 1;
        if pos < end && (bytes[pos] == b'+' || bytes[pos] == b'*' || bytes[pos] == b'?') {
            let kind = match bytes[pos] {
                b'+' => RepKind::Plus,
                b'*' => RepKind::Star,
                b'?' => RepKind::Question,
                _ => unreachable!(),
            };
            pos += 1;
            return Some((
                Some(Node::Repeat(Box::new(Node::Dot), kind)),
                pos,
            ));
        }
        return Some((Some(Node::Dot), i + 1));
    }

    // \d, \., \A, \z
    if b == b'\\' && i + 1 < end {
        let next = bytes[i + 1];
        match next {
            b'd' => {
                let mut pos = i + 2;
                if pos < end && (bytes[pos] == b'+' || bytes[pos] == b'*') {
                    let kind = match bytes[pos] {
                        b'+' => RepKind::Plus,
                        b'*' => RepKind::Star,
                        _ => unreachable!(),
                    };
                    pos += 1;
                    return Some((
                        Some(Node::Repeat(Box::new(Node::Digit), kind)),
                        pos,
                    ));
                }
                return Some((Some(Node::Digit), i + 2));
            }
            b'.' => return Some((Some(Node::Literal(b'.')), i + 2)),
            b'A' => return parse_node(bytes, i + 2, end),
            b'z' => return parse_node(bytes, i + 2, end),
            _ => return Some((Some(Node::Literal(next)), i + 2)),
        }
    }

    // Literal
    let mut pos = i + 1;
    if pos < end && (bytes[pos] == b'+' || bytes[pos] == b'*' || bytes[pos] == b'?') {
        let kind = match bytes[pos] {
            b'+' => RepKind::Plus,
            b'*' => RepKind::Star,
            b'?' => RepKind::Question,
            _ => unreachable!(),
        };
        pos += 1;
        return Some((
            Some(Node::Repeat(Box::new(Node::Literal(b)), kind)),
            pos,
        ));
    }
    Some((Some(Node::Literal(b)), i + 1))
}

/// Parse the inner content of a (...) group, return (inner_nodes, pos_after_close)
fn parse_group_inner(bytes: &[u8], start: usize, end: usize) -> Option<(Vec<Node>, usize)> {
    let mut nodes = Vec::new();
    let mut i = start;
    let mut depth = 1;
    while i < end && depth > 0 {
        let b = bytes[i];
        if b == b'(' {
            // Nested group — parse it as a node
            let (node, next) = parse_node(bytes, i, end)?;
            if let Some(n) = node {
                nodes.push(n);
            } else {
                depth += 1;
                nodes.push(Node::Literal(b'(')); // shouldn't happen
            }
            i = next;
            continue;
        }
        if b == b')' {
            depth -= 1;
            if depth == 0 {
                return Some((nodes, i + 1));
            }
            i += 1;
            continue;
        }
        let (node, next) = parse_node(bytes, i, end)?;
        if let Some(n) = node {
            nodes.push(n);
        }
        i = next;
    }
    if depth == 0 {
        Some((nodes, i))
    } else {
        None
    }
}

fn parse_char_class(bytes: &[u8], i: usize, end: usize) -> Option<(Option<Node>, usize)> {
    let mut j = i + 1; // skip [
    let negate = j < end && bytes[j] == b'^';
    if negate {
        j += 1;
    }
    let mut ranges = Vec::new();
    while j < end && bytes[j] != b']' {
        let c1 = bytes[j];
        // Check for range: a-b
        if j + 2 < end && bytes[j + 1] == b'-' && bytes[j + 2] != b']' {
            let c2 = bytes[j + 2];
            ranges.push((c1, c2));
            j += 3;
        } else {
            ranges.push((c1, c1));
            j += 1;
        }
    }
    if j >= end {
        return None; // unterminated
    }
    j += 1; // skip ]

    // Check for repetition
    let mut pos = j;
    if pos < end && (bytes[pos] == b'+' || bytes[pos] == b'*' || bytes[pos] == b'?') {
        let kind = match bytes[pos] {
            b'+' => RepKind::Plus,
            b'*' => RepKind::Star,
            b'?' => RepKind::Question,
            _ => unreachable!(),
        };
        pos += 1;
        return Some((
            Some(Node::Repeat(
                Box::new(Node::Class { ranges, negate }),
                kind,
            )),
            pos,
        ));
    }
    Some((Some(Node::Class { ranges, negate }), j))
}

/// Helper: join path components like Ruby's File.join
#[allow(dead_code)]
pub fn file_join(parts: &[&str]) -> String {
    parts.join("/")
}
