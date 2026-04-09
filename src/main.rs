mod diff;
mod render;

use std::process::Command;
use std::time::Instant;

use clap::Parser;
use anyhow::Result;

#[derive(Parser)]
#[command(name = "diffshot", version,
    about = "A tool to generate images from git diffs",
    author = "Faisal Fakih",
    )]
struct Args {
    /// Git diff target (e.g. main..feat/auth, HEAD~3). Defaults to uncommitted changes
    target: Option<String>,

    /// Restrict to git diffs of a specific file
    #[arg(long, short)]
    file: Option<String>,

    /// Output filename - extension determines format: png, jpg, jpeg, svg (default: diffshot.png)
    #[arg(long, short)]
    output: Option<String>,

    /// Directory to write output into (default: current directory)
    #[arg(long, short)]
    dir: Option<String>,

    /// Maximum number of lines to render (truncates with a footer if exceeded)
    #[arg(long, short='l')]
    max_lines: Option<usize>,

    /// Maximum number of lines to render per chunk/hunk (truncates each @@ block independently)
    #[arg(long, short='L')]
    max_lines_per_chunk: Option<usize>,

    /// Disable syntax highlighting
    #[arg(long)]
    no_highlight: bool,

    /// Compact mode: render all hunks of a file in one block instead of one block per chunk
    #[arg(long)]
    compact: bool,

    /// Pixel scale multiplier for output resolution (default: 2)
    #[arg(long, short, default_value_t = 2)]
    resolution: u32,

    /// Render each changed file as a separate image
    #[arg(long, short)]
    split: bool,
}

fn format_from_ext(filename: &str) -> render::Format {
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png"          => render::Format::Png,
        "jpg" | "jpeg" => render::Format::Jpeg,
        "svg"          => render::Format::Svg,
        other => {
            eprintln!("Unsupported file format '.{other}'. Use png, jpg, jpeg, or svg.");
            std::process::exit(1);
        }
    }
}

fn main() {
    let args = Args::parse();

    let filename = args.output.clone().unwrap_or_else(|| "diffshot.png".to_string());
    let output = match &args.dir {
        Some(dir) => format!("{}/{}", dir.trim_end_matches('/'), filename),
        None => filename.clone(),
    };

    let fmt = format_from_ext(&filename);

    // step 1: get the git diff
    let raw_diff = match get_git_diff(&args) {
        Ok(diff) => diff,
        Err(e) => {
            eprintln!("Error getting git diff: {}", e);
            std::process::exit(1);
        }
    };

    if raw_diff.trim().is_empty() {
        eprintln!("No changes found for the specified target.");
        std::process::exit(0);
    }

    // step 2: parse the diff into structured data
    let file_diffs = diff::parse_diff(&raw_diff);
    if file_diffs.is_empty() {
        eprintln!("No valid diffs found in the output.");
        std::process::exit(0);
    }

    // step 3: render the diff to SVG then PNG
    let start = Instant::now();

    if args.split {
        let ext = filename.rsplit('.').next().unwrap_or("png");
        let base_name = filename.rfind('.').map_or(filename.as_str(), |i| &filename[..i]);
        let base = match &args.dir {
            Some(dir) => format!("{}/{}", dir.trim_end_matches('/'), base_name),
            None => base_name.to_string(),
        };
        let mut total_added = 0;
        let mut total_removed = 0;

        for file in &file_diffs {
            let path = if let Some(dir) = &args.dir {
                // Preserve relative hierarchy: sanitize each path component individually
                // so that src/foo/bar.rs and src/foo-bar.rs remain distinct.
                let rel = std::path::Path::new(&file.filename);
                let stem = rel.file_name()
                    .map(|n| sanitize_filename(&n.to_string_lossy()))
                    .unwrap_or_else(|| sanitize_filename(&file.filename));
                let sanitized_parent: Option<String> = rel.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| {
                        p.components()
                            .map(|c| sanitize_filename(&c.as_os_str().to_string_lossy()))
                            .collect::<Vec<_>>()
                            .join("/")
                    });
                let out_dir = match sanitized_parent {
                    Some(ref parent) => {
                        let d = format!("{}/{}", dir.trim_end_matches('/'), parent);
                        if let Err(e) = std::fs::create_dir_all(&d) {
                            eprintln!("Error creating directory {d}: {e}");
                            std::process::exit(1);
                        }
                        d
                    }
                    None => dir.trim_end_matches('/').to_string(),
                };
                format!("{out_dir}/{base_name}-{stem}.{ext}")
            } else {
                // Flat layout: append a hash of the full relative path so that
                // src/foo/bar.rs and src/foo-bar.rs never produce the same filename.
                let hash = path_hash(&file.filename);
                format!("{base}-{}-{hash}.{ext}", sanitize_filename(&file.filename))
            };
            let (svg, stats) = render::render_svg(
                std::slice::from_ref(file),
                args.max_lines,
                args.max_lines_per_chunk,
                args.target.as_deref(),
                !args.no_highlight,
                args.compact,
            );
            if let Err(e) = render::render_to_file(&svg, &path, args.resolution, format_from_ext(&path)) {
                eprintln!("Error rendering {path}: {e}");
                std::process::exit(1);
            }
            total_added += stats.added;
            total_removed += stats.removed;
            println!("  Saved    {path}");
        }

        let elapsed = start.elapsed();
        let total = total_added + total_removed;
        println!(
            "  Time     {:.2}s\n  Changes  +{total_added} -{total_removed} ({total} lines)",
            elapsed.as_secs_f64(),
        );
    } else {
        let (svg, stats) = render::render_svg(&file_diffs, args.max_lines, args.max_lines_per_chunk, args.target.as_deref(), !args.no_highlight, args.compact);

        if let Err(e) = render::render_to_file(&svg, &output, args.resolution, fmt) {
            eprintln!("Error rendering image: {}", e);
            std::process::exit(1);
        }

        let elapsed = start.elapsed();
        let total = stats.added + stats.removed;
        let trunc_note = if stats.truncated { "  (truncated)" } else { "" };
        println!(
            "  Saved    {}\n  Time     {:.2}s\n  Changes  +{} -{} ({} lines){}",
            output,
            elapsed.as_secs_f64(),
            stats.added,
            stats.removed,
            total,
            trunc_note,
        );
    }
}


fn sanitize_filename(name: &str) -> String {
    name.replace('/', "-").replace('.', "_")
}

/// FNV-1a 32-bit hash - dependency-free and stable within a build.
fn path_hash(s: &str) -> String {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    format!("{h:08x}")
}

fn get_git_diff(args: &Args) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("diff").arg("--no-ext-diff").arg("--no-color").arg("--default-prefix");

    if let Some(target) = &args.target {
        cmd.arg(target);
    }

    if let Some(file) = &args.file {
        cmd.arg("--").arg(file);
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("git diff failed: {stderr}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
