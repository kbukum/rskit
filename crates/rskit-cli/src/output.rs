//! Structured terminal output — tables and key-value displays.

use std::fmt;

/// A formatted table for terminal output.
pub struct OutputTable {
    title: Option<String>,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl OutputTable {
    pub fn new(columns: Vec<impl Into<String>>) -> Self {
        Self {
            title: None,
            columns: columns.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn add_row(&mut self, row: Vec<impl Into<String>>) {
        self.rows.push(row.into_iter().map(Into::into).collect());
    }
}

impl fmt::Display for OutputTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut widths: Vec<usize> = self.columns.iter().map(|c| c.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        if let Some(title) = &self.title {
            writeln!(f, "\n{}", title)?;
        }

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┬");
        writeln!(f, "┌{}┐", separator)?;

        let header: String = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| format!(" {:width$} ", c, width = widths[i]))
            .collect::<Vec<_>>()
            .join("│");
        writeln!(f, "│{}│", header)?;

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┼");
        writeln!(f, "├{}┤", separator)?;

        for row in &self.rows {
            let cells: String = row
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let w = widths.get(i).copied().unwrap_or(0);
                    format!(" {:width$} ", c, width = w)
                })
                .collect::<Vec<_>>()
                .join("│");
            writeln!(f, "│{}│", cells)?;
        }

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┴");
        write!(f, "└{}┘", separator)?;

        Ok(())
    }
}

/// Key-value display for headers/summaries.
pub struct OutputKV {
    pairs: Vec<(String, String)>,
}

impl OutputKV {
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    pub fn add(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.pairs.push((key.into(), value.into()));
        self
    }
}

impl Default for OutputKV {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OutputKV {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let max_key = self.pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (key, value) in &self.pairs {
            writeln!(f, "  {:>width$}:  {}", key, value, width = max_key)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_table_renders() {
        let mut table = OutputTable::new(vec!["Name", "Count"]);
        table.add_row(vec!["real", "500"]);
        table.add_row(vec!["ai", "500"]);
        let output = table.to_string();
        assert!(output.contains("Name"));
        assert!(output.contains("500"));
    }

    #[test]
    fn output_kv_renders() {
        let mut kv = OutputKV::new();
        kv.add("Output", "/tmp/dataset");
        kv.add("Preset", "image");
        let output = kv.to_string();
        assert!(output.contains("Output"));
        assert!(output.contains("/tmp/dataset"));
    }
}
