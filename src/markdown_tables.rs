//! Align GitHub-Flavored Markdown pipe tables so that columns line up.
//!
//! This is a whitespace-only transformation: cell contents (including escaped
//! pipes) are preserved verbatim, only inter-cell padding changes. The number
//! of lines is always preserved exactly, so line numbers, line ranges and
//! per-line decorations remain correct.
//!
//! Tables are detected textually (a run of lines whose trimmed content starts
//! with `|`, where the second line is a `|---|---|` style separator), which
//! matches how table-aware formatters detect GFM tables. Fenced code blocks
//! are skipped. Anything that does not validate as a table is passed through
//! byte-for-byte.

use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

struct Line<'a> {
    /// Full original line, including the line terminator.
    raw: &'a str,
    /// Content without terminator.
    body: &'a str,
    /// Terminator ("", "\n" or "\r\n").
    suffix: &'a str,
}

fn split_line<'a>(line: &'a str) -> Line<'a> {
    let without_lf = line.strip_suffix('\n').unwrap_or(line);
    let (body, suffix) = match without_lf.strip_suffix('\r') {
        Some(body) => (body, &line[body.len()..]),
        None => (without_lf, &line[without_lf.len()..]),
    };
    Line {
        raw: line,
        body,
        suffix,
    }
}

fn indent_of(body: &str) -> &str {
    let end = body.len() - body.trim_start_matches([' ', '\t']).len();
    &body[..end]
}

fn is_table_row(line: &Line) -> bool {
    line.body.trim_start().starts_with('|')
}

/// Track fenced code blocks: returns Some((marker, length)) for an opening
/// fence line, and updates `fence` state.
fn update_fence(fence: &mut Option<(u8, usize)>, line: &Line) {
    let trimmed = line.body.trim();
    let marker = match trimmed.as_bytes().first() {
        Some(&b @ (b'`' | b'~')) => b,
        _ => return,
    };
    let run = trimmed.len() - trimmed.trim_start_matches(marker as char).len();
    match *fence {
        // Closing fence: same character, at least as long as the opener.
        Some((open, open_len))
            if open == marker && run >= open_len && trimmed[run..].trim().is_empty() =>
        {
            *fence = None;
        }
        Some(_) => {}
        None if run >= 3 => *fence = Some((marker, run)),
        None => {}
    }
}

/// Split a table row into cells. The line must start with `|` (after the
/// indent). Escaped pipes (`\|`) do not split cells.
fn parse_row<'a>(line: &Line<'a>) -> Option<(&'a str, Vec<&'a str>)> {
    let indent = indent_of(line.body);
    let rest = &line.body[indent.len()..];
    if !rest.starts_with('|') {
        return None;
    }
    let rest = &rest[1..];
    let rest = match rest.strip_suffix('|') {
        Some(r) if !r.ends_with('\\') => r,
        _ => rest,
    };
    let mut cells = vec![];
    let mut start = 0;
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'|' {
            i += 2; // escaped pipe, stays inside the cell
        } else if bytes[i] == b'|' {
            cells.push(rest[start..i].trim());
            start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    cells.push(rest[start..].trim());
    Some((indent, cells))
}

/// Parse a separator row like `| :--- | ---: |`, returning the per-column
/// alignment if every cell is a valid separator cell.
fn align_from_separator(cells: &[&str]) -> Option<Vec<(Align, bool, bool)>> {
    if cells.is_empty() {
        return None;
    }
    cells
        .iter()
        .map(|c| {
            let c = c.trim();
            let left = c.starts_with(':');
            let right = c.ends_with(':');
            let dashes = &c[usize::from(left)..c.len() - usize::from(right)];
            if dashes.is_empty() || !dashes.bytes().all(|b| b == b'-') {
                return None;
            }
            let align = match (left, right) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            };
            Some((align, left, right))
        })
        .collect()
}

fn pad(cell: &str, align: Align, width: usize) -> String {
    let len = cell.width();
    let (left, right) = match align {
        Align::Left => (0, width.saturating_sub(len)),
        Align::Right => (width.saturating_sub(len), 0),
        Align::Center => {
            let pad_total = width.saturating_sub(len);
            (pad_total / 2, pad_total - pad_total / 2)
        }
    };
    format!("{}{}{}", " ".repeat(left), cell, " ".repeat(right))
}

fn make_separator_cell(width: usize, left: bool, right: bool) -> String {
    let dashes = "-".repeat(width + 2 - usize::from(left) - usize::from(right));
    match (left, right) {
        (true, true) => format!(":{dashes}:"),
        (true, false) => format!(":{dashes}"),
        (false, true) => format!("{dashes}:"),
        (false, false) => dashes,
    }
}

/// Try to align one validated table block; returns None if it must be passed
/// through unchanged.
fn align_block(block: &[Line]) -> Option<Vec<String>> {
    if block.len() < 2 {
        return None;
    }
    // Tabs have terminal-dependent width; do not touch tables containing them.
    if block.iter().any(|l| l.body.contains('\t')) {
        return None;
    }
    let parsed: Vec<Option<(&str, Vec<&str>)>> = block.iter().map(parse_row).collect();
    if parsed.iter().any(Option::is_none) {
        return None;
    }
    let parsed: Vec<(&str, Vec<&str>)> = parsed.into_iter().flatten().collect();
    // Only realign blocks whose rows share one identical indent.
    let indent = parsed[0].0;
    if parsed.iter().any(|(ind, _)| *ind != indent) {
        return None;
    }
    let separators = align_from_separator(&parsed[1].1)?;
    let ncols = parsed.iter().map(|(_, c)| c.len()).max().unwrap_or(0);
    if separators.len() != parsed[1].1.len() || separators.len() > ncols {
        return None;
    }
    let mut separators = separators;
    separators.resize(ncols, (Align::Left, false, false));
    let aligns: Vec<Align> = separators.iter().map(|(a, _, _)| *a).collect();

    let widths: Vec<usize> = (0..ncols)
        .map(|col| {
            parsed
                .iter()
                .enumerate()
                // row 1 is the separator row; it never contributes to widths
                .filter(|(row_idx, _)| *row_idx != 1)
                .map(|(_, (_, cells))| cells.get(col).map_or(0, |c| c.width()))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut out = Vec::with_capacity(block.len());
    for (row_idx, (_, cells)) in parsed.iter().enumerate() {
        let suffix = block[row_idx].suffix;
        if row_idx == 1 {
            let sep: Vec<String> = (0..ncols)
                .map(|c| {
                    let (_, left, right) = separators[c];
                    make_separator_cell(widths[c], left, right)
                })
                .collect();
            out.push(format!("{indent}|{}|{suffix}", sep.join("|")));
        } else {
            let cells: Vec<String> = (0..ncols)
                .map(|c| pad(cells.get(c).copied().unwrap_or(""), aligns[c], widths[c]))
                .collect();
            out.push(format!("{indent}| {} |{suffix}", cells.join(" | ")));
        }
    }
    Some(out)
}

/// True for lines that end a block, meaning a table may start on the next
/// line: blank lines, ATX headings, fence markers, and table rows themselves.
/// (A plain paragraph line does *not* end a block — GFM tables cannot
/// interrupt a paragraph, and neither do we.)
fn ends_block(line: &Line) -> bool {
    let trimmed = line.body.trim_start();
    trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || trimmed.starts_with('|')
}

/// Align all GFM tables in `input`. Always returns exactly as many lines as
/// were given; non-table regions are passed through unchanged.
pub(crate) fn align_markdown_tables(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut fence: Option<(u8, usize)> = None;
    let mut lines = input.split_inclusive('\n').map(split_line).peekable();
    // Start of file is a block boundary.
    let mut at_boundary = true;

    while let Some(line) = lines.next() {
        if fence.is_some() || !is_table_row(&line) || !at_boundary {
            update_fence(&mut fence, &line);
            out.push_str(line.raw);
            at_boundary = ends_block(&line);
            continue;
        }
        // Collect a run of table rows.
        let mut block = vec![line];
        while let Some(next) = lines.peek() {
            if !is_table_row(next) {
                break;
            }
            block.push(lines.next().unwrap());
        }
        match align_block(&block) {
            Some(aligned) => {
                for l in &aligned {
                    out.push_str(l);
                }
            }
            None => {
                for l in &block {
                    out.push_str(l.raw);
                }
            }
        }
        // A table row ends a block.
        at_boundary = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aligned(s: &str) -> String {
        align_markdown_tables(s)
    }

    #[test]
    fn aligns_simple_table() {
        let input = "| a | bb |\n|---|---|\n| ccc | d |\n";
        let expected = "| a   | bb |\n|-----|----|\n| ccc | d  |\n";
        assert_eq!(aligned(input), expected);
    }

    #[test]
    fn preserves_line_count_and_passthrough() {
        let input = "hello\n| a | b |\n|---|---|\n| c | d |\nworld\n";
        let out = aligned(input);
        assert_eq!(out.lines().count(), input.lines().count());
        assert!(out.starts_with("hello\n"));
        assert!(out.ends_with("world\n"));
    }

    #[test]
    fn honors_alignment_colons() {
        let input = "| left | mid | right |\n|:-----|:---:|-----:|\n| x | yyyyy | z |\n";
        let out = aligned(input);
        assert!(
            out.contains("|:-----|:-----:|------:|"),
            "separator colons lost: {out}"
        );
        assert!(
            out.contains("| x    | yyyyy |     z |"),
            "padding ignores alignment: {out}"
        );
    }

    #[test]
    fn escaped_pipe_stays_in_cell() {
        let input = "| a \\| b | ccc |\n|---|---|\n| d | e |\n";
        let out = aligned(input);
        assert!(out.contains("| a \\| b | ccc |"), "got: {out}");
        assert!(out.contains("| d      | e   |"), "got: {out}");
    }

    #[test]
    fn pads_short_rows() {
        let input = "| long header | x |\n|---|---|\n| a |\n";
        let out = aligned(input);
        assert!(out.contains("| a           |   |"), "got: {out}");
    }

    #[test]
    fn skips_fenced_code_blocks() {
        let input =
            "```\n| a | b |\n|---|---|\n| c | d |\n```\n| eee | f |\n|---|---|\n| g | h |\n";
        let out = aligned(input);
        assert!(
            out.contains("| c | d |\n"),
            "code block was modified: {out}"
        );
        assert!(out.contains("| eee | f |\n|-----|---|"), "got: {out}");
    }

    #[test]
    fn lone_pipe_line_is_untouched() {
        let input = "| not a table\nplain text\n";
        assert_eq!(aligned(input), input);
    }

    #[test]
    fn crlf_terminators_preserved() {
        let input = "| a | bbb |\r\n|---|---|\r\n| c | d |\r\n";
        let out = aligned(input);
        assert_eq!(out, "| a | bbb |\r\n|---|-----|\r\n| c | d   |\r\n");
    }

    #[test]
    fn unicode_width_counts() {
        // em dash is one column wide
        let input = "| — | xx |\n|---|---|\n| aaa | b |\n";
        let out = aligned(input);
        assert!(out.contains("| —   | xx |"), "got: {out}");
        assert!(out.contains("| aaa | b  |"), "got: {out}");
    }

    #[test]
    fn tabs_in_table_skip_alignment() {
        let input = "| a\tb | c |\n|---|---|\n| d | e |\n";
        assert_eq!(aligned(input), input);
    }

    #[test]
    fn uniformly_indented_table_aligns() {
        let input = "  | a | bbb |\n  |---|---|\n  | c | d |\n";
        let out = aligned(input);
        assert!(
            out.starts_with("  | a | bbb |\n  |---|-----|"),
            "got: {out}"
        );
    }

    #[test]
    fn mixed_indent_table_passthrough() {
        let input = "  | a | b |\n|---|---|\n  | c | d |\n";
        assert_eq!(aligned(input), input);
    }

    #[test]
    fn table_at_eof_without_newline() {
        let input = "| a | bbb |\n|---|---|\n| c | d |";
        let out = aligned(input);
        assert_eq!(out, "| a | bbb |\n|---|-----|\n| c | d   |");
    }

    #[test]
    fn table_cannot_interrupt_paragraph() {
        // GFM: a pipe line directly after a paragraph line is paragraph text,
        // not a table — it must be passed through untouched.
        let input = "para\n| a | bbb |\n|---|---|\n| c | d |\n";
        assert_eq!(aligned(input), input);
    }

    #[test]
    fn table_after_blank_line_aligns() {
        let input = "para\n\n| a | bbb |\n|---|---|\n| c | d |\n";
        let out = aligned(input);
        assert!(out.contains("| a | bbb |\n|---|-----|"), "got: {out}");
    }

    #[test]
    fn table_after_heading_aligns() {
        let input = "## Head\n| a | bbb |\n|---|---|\n| c | d |\n";
        let out = aligned(input);
        assert!(out.contains("|---|-----|"), "got: {out}");
    }
}
