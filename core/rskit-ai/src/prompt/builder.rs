//! Prompt template builder.

use semver::Version;

use super::template::{PromptError, PromptTemplate, VariableDecl, VariableType};

/// Builder for [`PromptTemplate`].
#[derive(Debug, Clone)]
pub struct Builder {
    name: String,
    version: Version,
    body: Option<String>,
    variables: Vec<VariableDecl>,
    output_schema: Option<serde_json::Value>,
    description: String,
}

impl Builder {
    /// Create a new builder with a stable prompt name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: Version::new(0, 1, 0),
            body: None,
            variables: Vec::new(),
            output_schema: None,
            description: String::new(),
        }
    }

    /// Set the template version.
    #[must_use]
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// Set the template body.
    #[must_use]
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Add one required variable.
    #[must_use]
    pub fn variable(mut self, variable: impl Into<String>) -> Self {
        self.variables.push(VariableDecl {
            name: variable.into(),
            kind: VariableType::Any,
            required: true,
            default: None,
        });
        self
    }

    /// Set an optional output schema.
    #[must_use]
    pub fn output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Set a description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Build the template.
    pub fn build(self) -> Result<PromptTemplate, PromptError> {
        if self.name.trim().is_empty() {
            return Err(PromptError::MissingField("name"));
        }
        Ok(PromptTemplate {
            name: self.name,
            version: self.version,
            template: self.body.ok_or(PromptError::MissingField("body"))?,
            variables: self.variables,
            output_schema: self.output_schema,
            description: self.description,
        })
    }
}

impl PromptTemplate {
    /// Start building a template.
    pub fn builder(name: impl Into<String>) -> Builder {
        Builder::new(name)
    }
}
