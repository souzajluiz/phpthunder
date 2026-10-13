//! Built-in Markdown formatting on top of the whitespace clean-up: pipe
//! tables get their columns padded to a common width, so they read as a
//! grid in the source. Fenced code blocks are left alone.

/// Splits a table row into trimmed cells, ignoring `|` escaped with `\` or
/// inside inline code.
fn cells(row: &str) -> Vec<String> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = row
        .strip_suffix('|')
        .filter(|r| !r.ends_with('\\'))
        .unwrap_or(row);
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut in_code = false;
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                cell.push(c);
                if let Some(n) = chars.next() {
                    cell.push(n);
                }
            }
            '`' => {
                in_code = !in_code;
                cell.push(c);
            }
            '|' if !in_code => out.push(std::mem::take(&mut cell).trim().to_string()),
            _ => cell.push(c),
        }
    }
    out.push(cell.trim().to_string());
    out
}

/// Whether `row` is a table delimiter row (`| --- | :-: |`).
fn is_delimiter(row: &str) -> bool {
    row.contains('-')
        && cells(row).iter().all(|c| {
            let c = c.trim_start_matches(':').trim_end_matches(':');
            !c.is_empty() && c.chars().all(|ch| ch == '-')
        })
}

/// Re-renders one table (header, delimiter, body rows) with padded columns.
fn render(rows: &[&str]) -> Vec<String> {
    let parsed: Vec<Vec<String>> = rows.iter().map(|r| cells(r)).collect();
    let columns = parsed.iter().map(Vec::len).max().unwrap_or(0);
    let width = |s: &str| s.chars().count();
    let mut widths = vec![3usize; columns];
    for (i, row) in parsed.iter().enumerate() {
        if i == 1 {
            continue;
        }
        for (col, cell) in row.iter().enumerate() {
            widths[col] = widths[col].max(width(cell));
        }
    }
    let indent: String = rows[0].chars().take_while(|c| c.is_whitespace()).collect();
    parsed
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut line = format!("{indent}|");
            for (col, w) in widths.iter().enumerate() {
                let cell = row.get(col).map_or("", String::as_str);
                if i == 1 {
                    let left = cell.starts_with(':');
                    let right = cell.ends_with(':') && cell.len() > 1;
                    let dashes = w - usize::from(left) - usize::from(right);
                    line.push(' ');
                    line.push_str(if left { ":" } else { "" });
                    line.push_str(&"-".repeat(dashes));
                    line.push_str(if right { ":" } else { "" });
                    line.push_str(" |");
                } else {
                    line.push(' ');
                    line.push_str(cell);
                    line.push_str(&" ".repeat(w - width(cell)));
                    line.push_str(" |");
                }
            }
            line
        })
        .collect()
}

/// Pads the columns of every pipe table in `text` (a header row followed by
/// a delimiter row, then rows until a line without `|`). Fenced code blocks
/// are skipped. Line endings are `\n`.
pub fn align_tables(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut fence: Option<&str> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            out.push(line.to_string());
            i += 1;
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            out.push(line.to_string());
            i += 1;
            continue;
        }
        let is_table = line.contains('|') && lines.get(i + 1).is_some_and(|d| is_delimiter(d));
        if is_table {
            let mut end = i + 2;
            while end < lines.len() && lines[end].contains('|') && !lines[end].trim().is_empty() {
                end += 1;
            }
            out.extend(render(&lines[i..end]));
            i = end;
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_tables_should_pad_columns_and_keep_alignment_marks() {
        let src = "# Keys\n\n| Key | Default |\n|:-|-:|\n| `APP_ENV` | local |\n| `A\\|B` | x |\n\nafter\n";
        assert_eq!(
            align_tables(src),
            "# Keys\n\n| Key       | Default |\n| :-------- | ------: |\n| `APP_ENV` | local   |\n| `A\\|B`    | x       |\n\nafter\n"
        );
    }

    #[test]
    fn align_tables_should_skip_code_fences() {
        let src = "```\n| a | b |\n|-|-|\n```\n";
        assert_eq!(align_tables(src), src);
    }
}
