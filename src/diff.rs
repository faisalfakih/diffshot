/// A single changed file in the diff
#[derive(Debug)]
pub struct FileDiff {
    pub filename : String,
    pub diff : Vec<Hunk>,
}

/// Continuous change block of changed lines in a file diff
#[derive(Debug)]
pub struct Hunk {
    pub header: String, // e.g. "@@ -10,6 +10,8 @@"
    pub lines: Vec<DiffLine>,
}

/// A single line in a hunk, with its type (added, removed, unchanged)
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub content: String,
    pub line_type: LineType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineType {
    Added,
    Removed,
    Unchanged,
    /// Git "\ No newline at end of file" marker - not a real source line.
    Metadata,
}



/// Unescape a git-quoted path (e.g. `"src/h\303\251llo.rs"` -> `src/héllo.rs`).
/// If the string is not quoted, returns it unchanged.
fn unescape_git_path(s: &str) -> String {
    let s = s.trim();
    if !s.starts_with('"') || !s.ends_with('"') {
        return s.to_string();
    }
    let inner = &s[1..s.len() - 1];
    let mut out = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 1;
            match bytes[i] {
                b'n'  => { out.push(b'\n'); i += 1; }
                b't'  => { out.push(b'\t'); i += 1; }
                b'r'  => { out.push(b'\r'); i += 1; }
                b'\\' => { out.push(b'\\'); i += 1; }
                b'"'  => { out.push(b'"');  i += 1; }
                // Octal escape \NNN
                b'0'..=b'7' if i + 2 < bytes.len() => {
                    let octal = &inner.as_bytes()[i..i + 3];
                    if let (Some(a), Some(b), Some(c)) = (
                        (octal[0] as char).to_digit(8),
                        (octal[1] as char).to_digit(8),
                        (octal[2] as char).to_digit(8),
                    ) {
                        out.push((a * 64 + b * 8 + c) as u8);
                        i += 3;
                    } else {
                        out.push(b'\\'); // leave as-is
                    }
                }
                _ => { out.push(b'\\'); }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse raw unified diff output into structured FileDiff list
pub fn parse_diff(raw_diff: &str) -> Vec<FileDiff> {
    let mut file_diffs: Vec<FileDiff> = Vec::new();
    let mut current_file: Option<FileDiff> = None;
    let mut current_hunk: Option<Hunk> = None;

    for line in raw_diff.lines() {
        if line.starts_with("diff --git") {
            // flush hunk and file before starting a new one
            if let Some(hunk) = current_hunk.take() {
                if let Some(file) = &mut current_file {
                    file.diff.push(hunk);
                }
            }
            if let Some(file) = current_file.take() {
                file_diffs.push(file);
            }
            // Parse "diff --git a/<path> b/<path>" to seed filename immediately.
            // This handles deletions/renames where "+++ b/..." may never appear.
            let seeded_name = line
                .strip_prefix("diff --git ")
                .and_then(|rest| {
                    // rest is "a/<a_path> b/<b_path>"
                    let (a_part, b_part) = rest.split_once(" b/")?;
                    let b_path = b_part.trim();
                    if b_path == "/dev/null" || b_path.is_empty() {
                        // deletion: fall back to a/ path
                        a_part.strip_prefix("a/").map(|s| unescape_git_path(s.trim()))
                    } else {
                        Some(unescape_git_path(b_path))
                    }
                })
                .unwrap_or_default();
            current_file = Some(FileDiff {
                filename: seeded_name,
                diff: Vec::new(),
            });
        } else if line.starts_with("+++ b/") {
            if let Some(file) = &mut current_file {
                file.filename = unescape_git_path(line.trim_start_matches("+++ b/"));
            }
        } else if line.starts_with("@@") {
            if let Some(hunk) = current_hunk.take() {
                if let Some(file) = &mut current_file {
                    file.diff.push(hunk);
                }
            }
            current_hunk = Some(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
        } else if let Some(hunk) = &mut current_hunk {
            if line.starts_with("\\ No newline") {
                hunk.lines.push(DiffLine {
                    content: line.to_string(),
                    line_type: LineType::Metadata,
                });
            } else {
                let line_type = if line.starts_with('+') {
                    LineType::Added
                } else if line.starts_with('-') {
                    LineType::Removed
                } else {
                    LineType::Unchanged
                };
                hunk.lines.push(DiffLine {
                    content: if line.is_empty() { String::new() } else { line[1..].to_string() },
                    line_type,
                });
            }
        }
    }

    // Flush remaining hunk and file
    if let Some(hunk) = current_hunk.take() {
        if let Some(file) = &mut current_file {
            file.diff.push(hunk);
        }
    }
    if let Some(file) = current_file.take() {
        file_diffs.push(file);
    }

    file_diffs
}
