//! Static MCP resource registration, template matching, and dispatch helpers.

use std::{future::Future, pin::Pin, sync::Arc};

use rmcp::model::{ReadResourceRequestParams, ReadResourceResult, Resource, ResourceTemplate};

type ResourceFuture =
    Pin<Box<dyn Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send>>;

/// Static MCP resource registration.
pub struct ResourceEntry {
    /// Resource metadata exposed to clients.
    pub resource: Resource,
    pub(crate) handler: Arc<dyn Fn(ReadResourceRequestParams) -> ResourceFuture + Send + Sync>,
}

impl ResourceEntry {
    /// Construct a resource entry from resource metadata and an async handler.
    pub fn new<F, Fut>(resource: Resource, handler: F) -> Self
    where
        F: Fn(ReadResourceRequestParams) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send + 'static,
    {
        Self {
            resource,
            handler: Arc::new(move |request| Box::pin(handler(request))),
        }
    }
}

impl Clone for ResourceEntry {
    fn clone(&self) -> Self {
        Self {
            resource: self.resource.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

/// Static MCP resource-template registration.
pub struct ResourceTemplateEntry {
    /// Resource-template metadata exposed to clients.
    pub resource_template: ResourceTemplate,
    pub(crate) handler: Arc<dyn Fn(ReadResourceRequestParams) -> ResourceFuture + Send + Sync>,
}

impl ResourceTemplateEntry {
    /// Construct a resource-template entry from metadata and an async handler.
    pub fn new<F, Fut>(resource_template: ResourceTemplate, handler: F) -> Self
    where
        F: Fn(ReadResourceRequestParams) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send + 'static,
    {
        Self {
            resource_template,
            handler: Arc::new(move |request| Box::pin(handler(request))),
        }
    }
}

impl Clone for ResourceTemplateEntry {
    fn clone(&self) -> Self {
        Self {
            resource_template: self.resource_template.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

pub(crate) fn resource_uri(resource: &Resource) -> Option<String> {
    serde_json::to_value(resource).ok().and_then(|value| {
        value
            .get("uri")
            .and_then(|uri| uri.as_str())
            .map(str::to_string)
    })
}

pub(crate) fn resource_template_uri(resource_template: &ResourceTemplate) -> Option<String> {
    serde_json::to_value(resource_template)
        .ok()
        .and_then(|value| {
            value
                .get("uriTemplate")
                .and_then(|uri| uri.as_str())
                .map(str::to_string)
        })
}

pub(crate) fn resource_template_matches(template: &str, uri: &str) -> bool {
    let literals = template_literals(template);
    if literals.is_empty() {
        return template == uri;
    }
    let Some(first) = literals.first() else {
        return false;
    };
    if !uri.starts_with(first) {
        return false;
    }
    let mut index = first.len();
    for literal in literals.iter().skip(1) {
        if literal.is_empty() {
            continue;
        }
        let Some(found) = uri[index..].find(literal) else {
            return false;
        };
        index += found + literal.len();
    }
    if !template.ends_with('}')
        && let Some(last) = literals.last()
        && !last.is_empty()
    {
        return uri.ends_with(last);
    }
    true
}

fn template_literals(template: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for ch in template.chars() {
        match ch {
            '{' if depth == 0 => {
                literals.push(std::mem::take(&mut current));
                depth += 1;
            }
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => current.push(ch),
            _ => {}
        }
    }
    literals.push(current);
    literals
}
