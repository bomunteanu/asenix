use crate::api::rpc;
use crate::error::{MoteError, Result};
use crate::state::AppState;
use serde::Serialize;
use serde_json::json;

/// Resource definition
#[derive(Debug, Clone, Serialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// Resource template definition
#[derive(Debug, Clone, Serialize)]
pub struct ResourceTemplate {
    #[serde(rename = "uriTemplate")]
    pub uri_template: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

/// Resources list result
#[derive(Debug, Clone, Serialize)]
pub struct ResourcesListResult {
    pub resources: Vec<Resource>,
}

/// Resource templates list result
#[derive(Debug, Clone, Serialize)]
pub struct ResourceTemplatesListResult {
    #[serde(rename = "resourceTemplates")]
    pub templates: Vec<ResourceTemplate>,
}

/// Resource read result
#[derive(Debug, Clone, Serialize)]
pub struct ResourceReadResult {
    pub contents: Vec<ResourceContent>,
}

/// Resource content
#[derive(Debug, Clone, Serialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub text: String,
}

/// Get concrete resources (static URIs)
pub fn get_concrete_resources() -> ResourcesListResult {
    let resources = vec![
        Resource {
            uri: "fieldmap://all".to_string(),
            name: "Full Research Field Map".to_string(),
            description: "Synthesis atoms across all domains".to_string(),
            mime_type: "application/json".to_string(),
        },
    ];
    
    ResourcesListResult { resources }
}

/// Get resource templates (dynamic URIs)
pub fn get_resource_templates() -> ResourceTemplatesListResult {
    let templates = vec![
        ResourceTemplate {
            uri_template: "atom://{atom_id}".to_string(),
            name: "Research Atom".to_string(),
            description: "A single atom from the knowledge graph".to_string(),
            mime_type: "application/json".to_string(),
        },
        ResourceTemplate {
            uri_template: "artifact://{hash}/meta".to_string(),
            name: "Artifact Metadata".to_string(),
            description: "Metadata for a content-addressed artifact".to_string(),
            mime_type: "application/json".to_string(),
        },
        ResourceTemplate {
            uri_template: "fieldmap://{domain}".to_string(),
            name: "Research Field Map".to_string(),
            description: "Synthesis atoms representing current research state for a domain".to_string(),
            mime_type: "application/json".to_string(),
        },
    ];
    
    ResourceTemplatesListResult { templates }
}

/// Read a resource by URI
pub async fn read_resource(
    state: &AppState,
    uri: &str,
) -> Result<ResourceReadResult> {
    match uri {
        uri if uri.starts_with("atom://") => {
            let atom_id = uri
                .strip_prefix("atom://")
                .ok_or_else(|| MoteError::Validation("Invalid atom URI".to_string()))?;

            let atom = crate::db::queries::get_atom(&state.pool, atom_id).await?;
            let content = serde_json::to_string(&atom)
                .map_err(|e| MoteError::Validation(format!("Failed to serialize atom: {}", e)))?;

            Ok(ResourceReadResult {
                contents: vec![ResourceContent {
                    uri: uri.to_string(),
                    mime_type: "application/json".to_string(),
                    text: content,
                }],
            })
        }
        uri if uri.starts_with("artifact://") && uri.ends_with("/meta") => {
            let hash = uri
                .strip_prefix("artifact://")
                .and_then(|h| h.strip_suffix("/meta"))
                .ok_or_else(|| MoteError::Validation("Invalid artifact URI".to_string()))?;

            let metadata_result = crate::api::artifacts::get_artifact_metadata(
                axum::extract::State(std::sync::Arc::new(state.clone())),
                axum::extract::Path(hash.to_string()),
            )
            .await;

            let metadata = match metadata_result {
                Ok(meta) => meta,
                Err(status) => {
                    return Err(MoteError::Validation(format!(
                        "Failed to get metadata: {:?}",
                        status
                    )))
                }
            };

            let content = serde_json::to_string(&metadata.0)
                .map_err(|e| MoteError::Validation(format!("Failed to serialize metadata: {}", e)))?;

            Ok(ResourceReadResult {
                contents: vec![ResourceContent {
                    uri: uri.to_string(),
                    mime_type: "application/json".to_string(),
                    text: content,
                }],
            })
        }
        uri if uri.starts_with("fieldmap://") => {
            let domain = uri
                .strip_prefix("fieldmap://")
                .ok_or_else(|| MoteError::Validation("Invalid fieldmap URI".to_string()))?;

            let domain_param = if domain == "all" { None } else { Some(domain) };
            let atoms = rpc::handle_get_field_map(
                state,
                Some(json!({
                    "domain": domain_param,
                })),
            )
            .await?;

            let content = serde_json::to_string(&atoms)
                .map_err(|e| MoteError::Validation(format!("Failed to serialize atoms: {}", e)))?;

            Ok(ResourceReadResult {
                contents: vec![ResourceContent {
                    uri: uri.to_string(),
                    mime_type: "application/json".to_string(),
                    text: content,
                }],
            })
        }
        _ => Err(MoteError::Validation(format!("Unsupported resource URI: {}", uri))),
    }
}
