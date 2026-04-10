/// Truncate a string to max_len chars, appending `…` if truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = chars[..max_len.saturating_sub(1)].iter().collect();
        format!("{truncated}…")
    }
}

/// Build aligned columns from rows of strings.
/// Each row is a slice of cell values. Returns a Vec of formatted lines.
/// Columns are separated by `gap` spaces.
pub fn align_columns(rows: &[Vec<String>], gap: usize) -> Vec<String> {
    if rows.is_empty() {
        return vec![];
    }

    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; num_cols];

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let separator = " ".repeat(gap);
    rows.iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(i, cell)| {
                    if i == row.len() - 1 {
                        cell.to_string()
                    } else {
                        format!("{cell:<width$}", width = widths[i])
                    }
                })
                .collect::<Vec<_>>()
                .join(&separator)
                .trim_end()
                .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("hello world this is long", 10);
        assert_eq!(result, "hello wor…");
        assert_eq!(result.chars().count(), 10);
    }

    #[test]
    fn truncate_empty() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn align_columns_basic() {
        let rows = vec![
            vec!["a".into(), "bb".into(), "ccc".into()],
            vec!["dddd".into(), "e".into(), "ff".into()],
        ];
        let result = align_columns(&rows, 2);
        assert_eq!(result[0], "a     bb  ccc");
        assert_eq!(result[1], "dddd  e   ff");
    }

    #[test]
    fn align_columns_empty() {
        let rows: Vec<Vec<String>> = vec![];
        assert!(align_columns(&rows, 2).is_empty());
    }

    #[test]
    fn align_columns_single_column() {
        let rows = vec![vec!["hello".into()], vec!["world".into()]];
        let result = align_columns(&rows, 2);
        assert_eq!(result[0], "hello");
        assert_eq!(result[1], "world");
    }
}
