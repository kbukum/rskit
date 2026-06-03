use super::Placeholder;
use super::error::TemplateError;
use super::{Template, TemplatePart};
use std::fmt;

impl<P> Template<P>
where
    P: Placeholder,
{
    /// Render the template using a placeholder renderer callback.
    pub fn render_with<F, E>(&self, mut render: F) -> Result<String, TemplateError>
    where
        F: FnMut(P) -> Result<String, E>,
        E: fmt::Display,
    {
        let mut rendered = String::new();
        for part in &self.parts {
            match part {
                TemplatePart::Literal(value) => rendered.push_str(value),
                TemplatePart::Placeholder(placeholder) => {
                    let val = render(*placeholder)
                        .map_err(|error| TemplateError::Render(error.to_string()))?;
                    rendered.push_str(&val);
                }
            }
        }
        Ok(rendered)
    }
}
