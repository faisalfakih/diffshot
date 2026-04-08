use anyhow::Result;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use crate::diff::{FileDiff, Hunk, LineType};

// font
const FONT_B64: &str = include_str!("../fonts/JetBrainsMono.b64");
const FONT: &str = "JetBrains Mono, Fira Code, Menlo, Consolas, monospace";
const FONT_SIZE: u32 = 13;
const FONT_SMALL: u32 = 11;

// github dark palette
const OUTER_BG: &str = "#010409";
const CARD_BG: &str = "#0d1117";
const FILE_HDR_BG: &str = "#161b22";
const FILE_BORDER: &str = "#21262d";
const HUNK_HDR_BG: &str = "#1c2128";
const HUNK_HDR_FG: &str = "#8b949e";
const ADDED_BG: &str = "#1a3a2a";
const ADDED_FG: &str = "#3fb950";
const REMOVED_BG: &str = "#3a1a1a";
const REMOVED_FG: &str = "#f85149";
const UNCHANGED_FG: &str = "#c9d1d9";
const LINE_NUM_FG: &str = "#484f58";
const FILENAME_FG: &str = "#58a6ff";
const CHROME_FG: &str = "#8b949e";
const FOOTER_SEP: &str = "#21262d";
const FOOTER_DIM: &str = "#8b949e";
const FOOTER_BRAND: &str = "#484f58";
const DOT_RED: &str = "#ff5f57";
const DOT_YELLOW: &str = "#febc2e";
const DOT_GREEN: &str = "#28c840";

// truncated footer
const TRUNC_HDR_H: u32 = 26;
const TRUNC_FG: &str = "#8b949e";

// canvas & card
const CANVAS_W: u32 = 1000;
const OUTER_PAD: u32 = 20;
const CARD_W: u32 = CANVAS_W - 2 * OUTER_PAD; // 960
const CARD_PAD_X: u32 = 24;
const CARD_PAD_TOP: u32 = 20;
const CARD_PAD_BOTTOM: u32 = 24;
const CARD_RADIUS: u32 = 12;

// content area (inside card padding)
const CONTENT_X: u32 = OUTER_PAD + CARD_PAD_X; // 44
const CONTENT_W: u32 = CARD_W - 2 * CARD_PAD_X; // 912

// chrome row
const CHROME_H: u32 = 44; // height + gap to first file
const DOT_R: u32 = 6;

// file block
const FILE_RADIUS: u32 = 6;
const FILE_HDR_H: u32 = 34;
const FILE_GAP: u32 = 16;

// hunk & line
const HUNK_HDR_H: u32 = 24;
const LINE_H: u32 = 20;

// gutter: old line num col | new line num col | separator | code
const GUTTER_OLD_W: u32 = 36;
const GUTTER_NEW_W: u32 = 36;
const GUTTER_W: u32 = GUTTER_OLD_W + GUTTER_NEW_W; // 72
const GUTTER_PAD_R: u32 = 6; // right pad inside each col
const CODE_PAD: u32 = 14;

// derived x positions
const GUTTER_SEP_X: u32 = CONTENT_X + GUTTER_W; // 116
const OLD_NUM_X: u32 = CONTENT_X + GUTTER_OLD_W - GUTTER_PAD_R; // 74
const NEW_NUM_X: u32 = CONTENT_X + GUTTER_W - GUTTER_PAD_R; // 110
const CODE_X: u32 = GUTTER_SEP_X + CODE_PAD; // 130
const CODE_AREA_W: u32 = CONTENT_X + CONTENT_W - CODE_X; // 826px available for code text
const CHAR_W: u32 = 8; // conservative px-per-char for JetBrains Mono at FONT_SIZE 13
const MAX_CODE_CHARS: usize = (CODE_AREA_W / CHAR_W) as usize; // ~103

// footer
const FOOTER_H: u32 = 48;

// public types

pub struct RenderStats {
    pub added: usize,
    pub removed: usize,
    pub truncated: bool,
}

// main render entry point

pub fn render_svg(
    file_diffs: &[FileDiff],
    max_lines: Option<usize>,
    target: Option<&str>,
    highlight: bool,
) -> (String, RenderStats) {
    let mut elems: Vec<String> = Vec::with_capacity(file_diffs.len() * 64);
    let mut y = OUTER_PAD + CARD_PAD_TOP;
    let mut stats = RenderStats { added: 0, removed: 0, truncated: false };
    let mut lines_rendered: usize = 0;
    let mut total_lines_seen: usize = 0;
    let limit = max_lines.unwrap_or(usize::MAX);

    // load syntect once if highlighting is enabled
    let (ss, theme) = if highlight {
        let ss = SyntaxSet::load_defaults_nonewlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["base16-ocean.dark"].clone();
        (Some(ss), Some(theme))
    } else {
        (None, None)
    };

    // window chrome
    let chrome_label = target
        .map(|t| format!("diffshot {t}"))
        .unwrap_or_else(|| "diffshot".to_string());
    y += emit_chrome(&mut elems, y, &chrome_label);

    for file in file_diffs {
        // if limit already exhausted, only tally stats - don't render.
        if lines_rendered >= limit {
            for hunk in &file.diff {
                for dl in &hunk.lines {
                    total_lines_seen += 1;
                    match dl.line_type {
                        LineType::Added => stats.added += 1,
                        LineType::Removed => stats.removed += 1,
                        LineType::Unchanged | LineType::Metadata => {}
                    }
                }
            }
            continue;
        }

        // create a per-file highlighter (stateful - carries parse state across lines)
        let mut hl: Option<HighlightLines> = match (&ss, &theme) {
            (Some(ss), Some(theme)) => {
                let ext = file.filename.rsplit('.').next().unwrap_or("");
                let syntax = ss.find_syntax_by_extension(ext)
                    .unwrap_or_else(|| ss.find_syntax_plain_text());
                Some(HighlightLines::new(syntax, theme))
            }
            _ => None,
        };

        let file_start_y = y;
        y += emit_file_header(&mut elems, y, &file.filename);
        let code_start_y = y;

        for hunk in &file.diff {
            let (mut old_n, mut new_n) = parse_hunk_header(&hunk.header);

            if lines_rendered < limit {
                y += emit_hunk_header(&mut elems, y, hunk);
            }

            for dl in &hunk.lines {
                total_lines_seen += 1;
                match dl.line_type {
                    LineType::Added => stats.added += 1,
                    LineType::Removed => stats.removed += 1,
                    LineType::Unchanged | LineType::Metadata => {}
                }

                if lines_rendered >= limit {
                    // advance counters but don't render
                    match dl.line_type {
                        LineType::Added => { new_n += 1; }
                        LineType::Removed => { old_n += 1; }
                        LineType::Unchanged => { old_n += 1; new_n += 1; }
                        LineType::Metadata => {} // marker, no line numbers
                    }
                    continue;
                }

                if dl.line_type == LineType::Metadata {
                    lines_rendered += 1;
                    y += emit_metadata_line(&mut elems, y, &dl.content);
                    continue;
                }

                let (old_s, new_s, prefix, bg, fg) = match dl.line_type {
                    LineType::Added => {
                        let s = new_n.to_string();
                        new_n += 1;
                        (String::new(), s, "+", ADDED_BG, ADDED_FG)
                    }
                    LineType::Removed => {
                        let s = old_n.to_string();
                        old_n += 1;
                        (s, String::new(), "-", REMOVED_BG, REMOVED_FG)
                    }
                    LineType::Unchanged => {
                        let o = old_n.to_string();
                        let n = new_n.to_string();
                        old_n += 1;
                        new_n += 1;
                        (o, n, "", CARD_BG, UNCHANGED_FG)
                    }
                    LineType::Metadata => unreachable!(),
                };

                // prefix takes 2 chars ("+ " / "- "), reduce budget accordingly
                let prefix_chars = if prefix.is_empty() { 0 } else { 2 };
                let content = truncate_line(&dl.content, MAX_CODE_CHARS.saturating_sub(prefix_chars));

                let hl_ctx = ss.as_ref().zip(hl.as_mut());
                let code_svg = build_code_line(hl_ctx, prefix, fg, &content);

                let by = y + text_baseline(LINE_H, FONT_SIZE);
                rect(&mut elems, CONTENT_X, y, CONTENT_W, LINE_H, bg);

                if !old_s.is_empty() {
                    elems.push(format!(
                        r#"<text x="{OLD_NUM_X}" y="{by}" text-anchor="end" fill="{LINE_NUM_FG}" font-family="{FONT}" font-size="{FONT_SMALL}">{old_s}</text>"#
                    ));
                }
                if !new_s.is_empty() {
                    elems.push(format!(
                        r#"<text x="{NEW_NUM_X}" y="{by}" text-anchor="end" fill="{LINE_NUM_FG}" font-family="{FONT}" font-size="{FONT_SMALL}">{new_s}</text>"#
                    ));
                }
                if !code_svg.is_empty() {
                    elems.push(format!(
                        r#"<text x="{CODE_X}" y="{by}" fill="{fg}" font-family="{FONT}" font-size="{FONT_SIZE}" xml:space="preserve">{code_svg}</text>"#
                    ));
                }

                y += LINE_H;
                lines_rendered += 1;
            }
        }

        let file_end_y = y;

        // gutter separator - vertical line through code area only
        elems.push(format!(
            r#"<line x1="{GUTTER_SEP_X}" y1="{code_start_y}" x2="{GUTTER_SEP_X}" y2="{file_end_y}" stroke="{FILE_BORDER}" stroke-width="1"/>"#
        ));

        // header / code body separator
        elems.push(format!(
            r#"<line x1="{CONTENT_X}" y1="{code_start_y}" x2="{}" y2="{code_start_y}" stroke="{FILE_BORDER}" stroke-width="1"/>"#,
            CONTENT_X + CONTENT_W
        ));

        // border overlay drawn last so it sits on top of content
        let file_h = file_end_y - file_start_y;
        elems.push(format!(
            r#"<rect x="{CONTENT_X}" y="{file_start_y}" width="{CONTENT_W}" height="{file_h}" rx="{FILE_RADIUS}" fill="none" stroke="{FILE_BORDER}" stroke-width="1"/>"#
        ));

        y += FILE_GAP;
    }

    // truncated notice
    if lines_rendered < total_lines_seen {
        stats.truncated = true;
        let skipped = total_lines_seen - lines_rendered;
        y += emit_truncated_footer(&mut elems, y, skipped);
    }

    // stats footer
    y += emit_footer(&mut elems, y, &stats, file_diffs.len());

    y += CARD_PAD_BOTTOM;

    // compose final SVG
    let total_h = y + OUTER_PAD;
    let card_h = total_h - 2 * OUTER_PAD;

    let defs = format!(
        r#"<defs><style>@font-face {{
  font-family: 'JetBrains Mono';
  src: url('data:font/woff2;base64,{}');
}}</style></defs>"#,
        FONT_B64.trim()
    );

    let bg_rects = format!(
        r#"<rect width="{CANVAS_W}" height="{total_h}" fill="{OUTER_BG}"/>
<rect x="{OUTER_PAD}" y="{OUTER_PAD}" width="{CARD_W}" height="{card_h}" rx="{CARD_RADIUS}" fill="{CARD_BG}"/>"#
    );

    let body = elems.join("\n");

    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS_W}" height="{total_h}" viewBox="0 0 {CANVAS_W} {total_h}">
{defs}
{bg_rects}
{body}
</svg>"#
    );

    (svg, stats)
}

// output format

pub enum Format {
    Png,
    Jpeg,
    Svg,
}

// export

pub fn render_to_file(svg_str: &str, output_path: &str, scale: u32, format: Format) -> Result<()> {
    if matches!(format, Format::Svg) {
        return std::fs::write(output_path, svg_str)
            .map_err(|e| anyhow::anyhow!("SVG write error: {e}"));
    }

    use resvg::{tiny_skia, usvg};

    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    opt.fontdb_mut().load_font_data(
        include_bytes!("../fonts/JetBrainsMono-Regular.woff2").to_vec()
    );

    let tree = usvg::Tree::from_str(svg_str, &opt)
        .map_err(|e| anyhow::anyhow!("SVG parse error: {e}"))?;

    let size = tree.size().to_int_size();
    let pw = size.width()
        .checked_mul(scale)
        .filter(|&v| v > 0)
        .ok_or_else(|| anyhow::anyhow!(
            "Invalid output width: {}×{} overflows or is zero", size.width(), scale
        ))?;
    let ph = size.height()
        .checked_mul(scale)
        .filter(|&v| v > 0)
        .ok_or_else(|| anyhow::anyhow!(
            "Invalid output height: {}×{} overflows or is zero", size.height(), scale
        ))?;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph)
        .ok_or_else(|| anyhow::anyhow!("Failed to allocate pixmap ({}×{})", pw, ph))?;

    resvg::render(&tree, tiny_skia::Transform::from_scale(scale as f32, scale as f32), &mut pixmap.as_mut());

    match format {
        Format::Png => pixmap
            .save_png(output_path)
            .map_err(|e| anyhow::anyhow!("PNG write error: {e}"))?,
        Format::Jpeg => {
            let w = pixmap.width();
            let h = pixmap.height();
            // tiny-skia stores premultiplied RGBA; background is fully opaque so drop alpha
            let rgb: Vec<u8> = pixmap.data().chunks(4).flat_map(|px| [px[0], px[1], px[2]]).collect();
            let img = image::RgbImage::from_raw(w, h, rgb)
                .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;
            img.save_with_format(output_path, image::ImageFormat::Jpeg)
                .map_err(|e| anyhow::anyhow!("JPEG write error: {e}"))?;
        }
        Format::Svg => unreachable!(),
    }

    Ok(())
}

// emit helpers

fn emit_chrome(out: &mut Vec<String>, y: u32, label: &str) -> u32 {
    let cy = y + DOT_R;
    let x0 = CONTENT_X;
    out.push(format!(r#"<circle cx="{x0}" cy="{cy}" r="{DOT_R}" fill="{DOT_RED}"/>"#));
    out.push(format!(r#"<circle cx="{}" cy="{cy}" r="{DOT_R}" fill="{DOT_YELLOW}"/>"#, x0 + 20));
    out.push(format!(r#"<circle cx="{}" cy="{cy}" r="{DOT_R}" fill="{DOT_GREEN}"/>"#, x0 + 40));

    let tx = x0 + 64;
    let ty = cy + 4; // visually center with dots
    out.push(format!(
        r#"<text x="{tx}" y="{ty}" fill="{CHROME_FG}" font-family="{FONT}" font-size="12">{}</text>"#,
        xml_escape(label)
    ));

    CHROME_H
}

fn emit_file_header(out: &mut Vec<String>, y: u32, filename: &str) -> u32 {
    // rounded-top rect for the header background
    out.push(rounded_top_rect(CONTENT_X, y, CONTENT_W, FILE_HDR_H, FILE_RADIUS, FILE_HDR_BG));

    let ty = y + text_baseline(FILE_HDR_H, FONT_SIZE);
    out.push(format!(
        r#"<text x="{}" y="{ty}" fill="{FILENAME_FG}" font-family="{FONT}" font-size="{FONT_SIZE}" font-weight="600">&#9679; {}</text>"#,
        CONTENT_X + 14,
        xml_escape(filename)
    ));

    FILE_HDR_H
}

fn emit_hunk_header(out: &mut Vec<String>, y: u32, hunk: &Hunk) -> u32 {
    rect(out, CONTENT_X, y, CONTENT_W, HUNK_HDR_H, HUNK_HDR_BG);
    let ty = y + text_baseline(HUNK_HDR_H, FONT_SMALL);
    out.push(format!(
        r#"<text x="{CODE_X}" y="{ty}" fill="{HUNK_HDR_FG}" font-family="{FONT}" font-size="{FONT_SMALL}">{}</text>"#,
        xml_escape(&hunk.header)
    ));
    HUNK_HDR_H
}

fn emit_metadata_line(out: &mut Vec<String>, y: u32, text: &str) -> u32 {
    rect(out, CONTENT_X, y, CONTENT_W, LINE_H, CARD_BG);
    let ty = y + text_baseline(LINE_H, FONT_SMALL);
    out.push(format!(
        r#"<text x="{CODE_X}" y="{ty}" fill="{LINE_NUM_FG}" font-family="{FONT}" font-size="{FONT_SMALL}" font-style="italic">{}</text>"#,
        xml_escape(text)
    ));
    LINE_H
}

fn emit_truncated_footer(out: &mut Vec<String>, y: u32, skipped: usize) -> u32 {
    rect(out, CONTENT_X, y, CONTENT_W, TRUNC_HDR_H, HUNK_HDR_BG);
    let ty = y + text_baseline(TRUNC_HDR_H, FONT_SMALL);
    let s = if skipped == 1 { "" } else { "s" };
    let msg = format!("... {skipped} more line{s} not shown");
    out.push(format!(
        r#"<text x="{CODE_X}" y="{ty}" fill="{TRUNC_FG}" font-family="{FONT}" font-size="{FONT_SMALL}" font-style="italic">{msg}</text>"#
    ));
    TRUNC_HDR_H
}

fn emit_footer(out: &mut Vec<String>, y: u32, stats: &RenderStats, num_files: usize) -> u32 {
    let sep_y = y + 16;
    let right_x = CONTENT_X + CONTENT_W;

    out.push(format!(
        r#"<line x1="{CONTENT_X}" y1="{sep_y}" x2="{right_x}" y2="{sep_y}" stroke="{FOOTER_SEP}" stroke-width="1"/>"#
    ));

    let ty = sep_y + 20;

    let add_s = if stats.added == 1 { "" } else { "s" };
    let rem_s = if stats.removed == 1 { "" } else { "s" };
    let fil_s = if num_files == 1 { "" } else { "s" };
    let additions = xml_escape(&format!("+{} addition{add_s}", stats.added));
    let deletions = xml_escape(&format!("-{} deletion{rem_s}", stats.removed));
    let files = xml_escape(&format!("{num_files} file{fil_s} changed"));

    out.push(format!(
        r#"<text x="{CONTENT_X}" y="{ty}" font-family="{FONT}" font-size="12"><tspan fill="{ADDED_FG}">{additions}</tspan><tspan dx="16" fill="{REMOVED_FG}">{deletions}</tspan><tspan dx="16" fill="{FOOTER_DIM}">{files}</tspan></text>"#
    ));

    out.push(format!(
        r#"<text x="{right_x}" y="{ty}" text-anchor="end" fill="{FOOTER_BRAND}" font-family="{FONT}" font-size="11">generated by diffshot</text>"#
    ));

    FOOTER_H
}

// highlighting

// expands tabs and truncates to max_chars, appending … if clipped.
fn truncate_line(content: &str, max_chars: usize) -> String {
    let expanded = content.replace('\t', "    ");
    if expanded.chars().count() <= max_chars {
        expanded
    } else {
        expanded.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…"
    }
}

// builds the SVG content string for one code line.
// with highlighting: a series of <tspan fill="..."> elements per token.
// without: plain escaped text.
// the +/- prefix (if any) is always rendered in the line's fg color.
fn build_code_line(
    hl: Option<(&SyntaxSet, &mut HighlightLines)>,
    prefix: &str,
    fg: &str,
    content: &str,
) -> String {
    let body = match hl {
        Some((ss, hl)) => match hl.highlight_line(content, ss) {
            Ok(regions) => regions_to_svg(&regions),
            Err(_) => xml_escape(&content.replace('\t', "    ")),
        },
        None => xml_escape(&content.replace('\t', "    ")),
    };

    if prefix.is_empty() {
        body
    } else {
        format!(r#"<tspan fill="{fg}">{prefix} </tspan>{body}"#)
    }
}

fn regions_to_svg(regions: &[(Style, &str)]) -> String {
    let mut out = String::new();
    for (style, text) in regions {
        if text.is_empty() {
            continue;
        }
        let color = format!(
            "#{:02x}{:02x}{:02x}",
            style.foreground.r, style.foreground.g, style.foreground.b
        );
        let escaped = xml_escape(&text.replace('\t', "    "));
        out.push_str(&format!(r#"<tspan fill="{color}">{escaped}</tspan>"#));
    }
    out
}

// geometry helpers

// svg path for a rect with rounded top corners only
fn rounded_top_rect(x: u32, y: u32, w: u32, h: u32, r: u32, fill: &str) -> String {
    let x2 = x + w;
    let y2 = y + h;
    format!(
        r#"<path d="M {},{} H {} Q {},{} {},{} V {} H {} V {} Q {},{} {},{} Z" fill="{}"/>"#,
        x + r, y,         // M - start after top-left arc
        x2 - r,           // H - across top
        x2, y, x2, y + r, // Q - top-right arc
        y2,               // V - right side down
        x,                // H - bottom edge
        y + r,            // V - left side up
        x, y, x + r, y,  // Q - top-left arc
        fill
    )
}

fn text_baseline(row_h: u32, font_size: u32) -> u32 {
    (row_h + font_size) / 2 - 1
}

fn rect(out: &mut Vec<String>, x: u32, y: u32, w: u32, h: u32, fill: &str) {
    out.push(format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#
    ));
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn parse_hunk_header(header: &str) -> (u32, u32) {
    let mut old_start = 1u32;
    let mut new_start = 1u32;

    if let Some(rest) = header.strip_prefix("@@ ") {
        let mut parts = rest.split_whitespace();
        if let Some(old_part) = parts.next() {
            if let Some(s) = old_part.strip_prefix('-') {
                old_start = s.split(',').next().unwrap_or("1").parse().unwrap_or(1);
            }
        }
        if let Some(new_part) = parts.next() {
            if let Some(s) = new_part.strip_prefix('+') {
                new_start = s.split(',').next().unwrap_or("1").parse().unwrap_or(1);
            }
        }
    }

    (old_start, new_start)
}
