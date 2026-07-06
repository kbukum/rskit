//! Key-value display for headers and summaries.

use std::fmt;

/// Key-value display for headers/summaries.
pub struct OutputKV {
    pairs: Vec<(String, String)>,
}

impl OutputKV {
    /// Create an empty key-value output block.
    #[must_use]
    pub const fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Add a key-value pair to the output block.
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
        let max_key = self
            .pairs
            .iter()
            .map(|(k, _)| k.chars().count())
            .max()
            .unwrap_or(0);
        for (key, value) in &self.pairs {
            writeln!(f, "  {key:>max_key$}:  {value}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::OutputKV;

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
