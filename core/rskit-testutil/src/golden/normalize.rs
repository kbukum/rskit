use regex::{NoExpand, Regex};
use rskit_errors::{AppError, AppResult};

/// How a [`Rule`] locates the spans to rewrite.
#[derive(Debug, Clone)]
enum RuleMatcher {
    Literal(String),
    Pattern(Regex),
}

/// One ordered substitution rule: a matched span becomes a stable placeholder.
///
/// Rules are supplied by the caller (an absolute temp path → `<ROOT>`, a
/// duration → `<DUR>`, a hex digest → `<HASH>`), keeping the normalizer free of
/// any domain knowledge. Placeholders are inserted literally — no capture-group
/// expansion.
#[derive(Debug, Clone)]
pub struct Rule {
    matcher: RuleMatcher,
    placeholder: String,
}

impl Rule {
    /// A rule replacing every occurrence of the literal `text`.
    #[must_use]
    pub fn literal(text: impl Into<String>, placeholder: impl Into<String>) -> Self {
        Self {
            matcher: RuleMatcher::Literal(text.into()),
            placeholder: placeholder.into(),
        }
    }

    /// A rule replacing every match of the regex `pattern`.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AppError`] (cause preserved) when `pattern` is not a valid regex.
    pub fn pattern(pattern: &str, placeholder: impl Into<String>) -> AppResult<Self> {
        let regex = Regex::new(pattern).map_err(|err| {
            AppError::invalid_input("pattern", "failed to compile normalization pattern")
                .with_cause(err)
        })?;
        Ok(Self {
            matcher: RuleMatcher::Pattern(regex),
            placeholder: placeholder.into(),
        })
    }

    fn apply(&self, input: &str) -> String {
        match &self.matcher {
            RuleMatcher::Literal(text) => input.replace(text, &self.placeholder),
            RuleMatcher::Pattern(regex) => regex
                .replace_all(input, NoExpand(&self.placeholder))
                .into_owned(),
        }
    }
}

/// An ordered list of substitution [`Rule`]s applied to raw output.
///
/// Rules run in the order given: a span rewritten by an earlier rule is no
/// longer visible to later ones, so callers order rules from most to least
/// specific.
#[derive(Debug, Clone, Default)]
pub struct Normalizer {
    rules: Vec<Rule>,
}

impl Normalizer {
    /// A normalizer applying `rules` in order.
    #[must_use]
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    /// Rewrite `input` by applying every rule in order.
    #[must_use]
    pub fn apply(&self, input: &str) -> String {
        self.rules
            .iter()
            .fold(input.to_owned(), |text, rule| rule.apply(&text))
    }
}
