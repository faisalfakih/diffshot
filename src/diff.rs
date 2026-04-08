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
            current_file = Some(FileDiff {
                filename: String::new(), // filled in when we hit +++ line
                diff: Vec::new(),
            });
        } else if line.starts_with("+++ b/") {
            if let Some(file) = &mut current_file {
                file.filename = line.trim_start_matches("+++ b/").to_string();
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
