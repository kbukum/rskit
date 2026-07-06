//! A formatted table for terminal output.

use std::fmt;

/// A formatted table for terminal output.
pub struct OutputTable {
    title: Option<String>,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl OutputTable {
    /// Create a table with the given column headings.
    #[must_use]
    pub fn new(columns: Vec<impl Into<String>>) -> Self {
        Self {
            title: None,
            columns: columns.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    /// Set a human-readable table title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Append a row of cell values.
    pub fn add_row(&mut self, row: Vec<impl Into<String>>) {
        self.rows.push(row.into_iter().map(Into::into).collect());
    }
}

impl fmt::Display for OutputTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut widths: Vec<usize> = self.columns.iter().map(|c| c.chars().count()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.chars().count());
                }
            }
        }

        if let Some(title) = &self.title {
            writeln!(f, "\n{title}")?;
        }

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┬");
        writeln!(f, "┌{separator}┐")?;

        let header: String = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| format!(" {:width$} ", c, width = widths[i]))
            .collect::<Vec<_>>()
            .join("│");
        writeln!(f, "│{header}│")?;

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┼");
        writeln!(f, "├{separator}┤")?;

        for row in &self.rows {
            let cells: String = row
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let w = widths.get(i).copied().unwrap_or(0);
                    format!(" {c:w$} ")
                })
                .collect::<Vec<_>>()
                .join("│");
            writeln!(f, "│{cells}│")?;
        }

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┴");
        write!(f, "└{separator}┘")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::OutputTable;

    #[test]
    fn output_table_renders() {
        let mut table = OutputTable::new(vec!["Name", "Count"]);
        table.add_row(vec!["real", "500"]);
        table.add_row(vec!["ai", "500"]);
        let output = table.to_string();
        assert!(output.contains("Name"));
        assert!(output.contains("500"));
    }
}
