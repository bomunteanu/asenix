use crate::api::rpc;
use crate::error::{MoteError, Result};
use serde::Serialize;
use serde_json::{json, Value};

/// Tool definition
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Tools list result
#[derive(Debug, Clone, Serialize)]
pub struct ToolsListResult {
    pub tools: Vec<Tool>,
}

/// Get all available tools
pub fn get_all_tools() -> ToolsListResult {
    let tools = vec![
        Tool {
            name: "register_agent".to_string(),
            description: "Register a new agent with an Ed25519 public key. Returns agent ID and authentication challenge.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "public_key": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 public key"
                    }
                },
                "required": ["public_key"]
            }),
        },
        Tool {
            name: "confirm_agent".to_string(),
            description: "Confirm agent identity by signing authentication challenge.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID returned from registration"
                    },
                    "signature": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 signature of challenge"
                    }
                },
                "required": ["agent_id", "signature"]
            }),
        },
        Tool {
            name: "publish_atoms".to_string(),
            description: "Publish one or more research atoms to knowledge graph. Requires authenticated agent.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Authenticated agent ID"
                    },
                    "signature": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 signature for authentication"
                    },
                    "atoms": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "atom_type": {
                                    "type": "string",
                                    "enum": ["hypothesis", "finding", "negative_result", "delta", "experiment_log", "synthesis", "bounty"],
                                    "description": "Type of research atom"
                                },
                                "domain": {
                                    "type": "string",
                                    "description": "Research domain"
                                },
                                "statement": {
                                    "type": "string",
                                    "description": "Research statement or claim"
                                },
                                "conditions": {
                                    "type": "object",
                                    "description": "Experimental conditions and parameters"
                                },
                                "metrics": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "description": "Performance metrics and measurements"
                                    },
                                    "nullable": true
                                },
                                "provenance": {
                                    "type": "object",
                                    "description": "Source and methodological information"
                                },
                                "artifact_tree_hash": {
                                    "type": "string",
                                    "description": "Hash of associated artifact tree",
                                    "nullable": true
                                },
                                "signature": {
                                    "type": "string",
                                    "description": "Hex-encoded Ed25519 signature for authentication"
                                }
                            },
                            "required": ["atom_type", "domain", "statement", "signature"]
                        }
                    },
                    "edges": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "source_atom_id": {
                                    "type": "string",
                                    "description": "ID of source atom"
                                },
                                "target_atom_id": {
                                    "type": "string",
                                    "description": "ID of target atom"
                                },
                                "edge_type": {
                                    "type": "string",
                                    "enum": ["supports", "contradicts", "extends", "retracts"]
                                }
                            },
                            "nullable": true
                        }
                    }
                },
                "required": ["agent_id", "signature", "atoms"]
            }),
        },
        Tool {
            name: "search_atoms".to_string(),
            description: "Search knowledge graph using filters.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "domain": {
                        "type": "string",
                        "description": "Filter by research domain"
                    },
                    "type": {
                        "type": "string",
                        "description": "Filter by atom type"
                    },
                    "lifecycle": {
                        "type": "string",
                        "description": "Filter by publication status"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Number of results to skip for pagination"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "query_cluster".to_string(),
            description: "Find atoms near a point in embedding space with pheromone landscape.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vector": {
                        "type": "array",
                        "items": {
                            "type": "number",
                            "description": "Embedding vector for cluster center"
                        }
                    },
                    "radius": {
                        "type": "number",
                        "description": "Search radius for cluster"
                    }
                },
                "required": ["vector", "radius"]
            }),
        },
        Tool {
            name: "claim_direction".to_string(),
            description: "Claim a research direction. Returns neighbourhood, active claims, and pheromone landscape. Requires authenticated agent.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Authenticated agent ID"
                    },
                    "signature": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 signature for authentication"
                    },
                    "hypothesis": {
                        "type": "string",
                        "description": "Research hypothesis to claim"
                    },
                    "conditions": {
                        "type": "object",
                        "description": "Experimental conditions and parameters"
                    },
                    "domain": {
                        "type": "string",
                        "description": "Research domain"
                    }
                },
                "required": ["agent_id", "signature", "hypothesis", "conditions", "domain"]
            }),
        },
        Tool {
            name: "retract_atom".to_string(),
            description: "Retract a previously published atom. Only publishing agent can retract. Requires authenticated agent.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Authenticated agent ID"
                    },
                    "signature": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 signature for authentication"
                    },
                    "atom_id": {
                        "type": "string",
                        "description": "ID of atom to retract"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Reason for retraction"
                    }
                },
                "required": ["agent_id", "signature", "atom_id", "reason"]
            }),
        },
        Tool {
            name: "get_suggestions".to_string(),
            description: "Get ranked research direction suggestions based on pheromone landscape.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "domain": {
                        "type": "string",
                        "description": "Research domain (optional)",
                        "nullable": true
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of suggestions to return"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "get_field_map".to_string(),
            description: "Get synthesis atoms representing current state of research in a domain.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "domain": {
                        "type": "string",
                        "description": "Research domain (optional)"
                    }
                },
                "required": []
            }),
        },
    ];
    
    ToolsListResult { tools }
}

/// Call a specific tool by name
pub async fn call_tool(
    state: &crate::state::AppState,
    tool_name: &str,
    arguments: &Value,
) -> Result<Value> {
    match tool_name {
        "register_agent" => {
            // Extract parameters
            let public_key = arguments.get("public_key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing public_key parameter".to_string()))?;
            
            // Call handler
            rpc::handle_register_agent(state, Some(json!({ "public_key": public_key }))).await
        },
        "confirm_agent" => {
            let agent_id = arguments.get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing agent_id parameter".to_string()))?;
            
            let signature = arguments.get("signature")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing signature parameter".to_string()))?;
            
            // Call handler
            rpc::handle_confirm_agent(state, Some(json!({
                "agent_id": agent_id,
                "signature": signature,
            }))).await
        },
        "publish_atoms" => {
            // Extract agent_id and signature for authentication
            let agent_id = arguments.get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing agent_id parameter".to_string()))?;
            
            let signature = arguments.get("signature")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing signature parameter".to_string()))?;
            
            // Extract atoms array
            let atoms_value = arguments.get("atoms")
                .ok_or_else(|| MoteError::Validation("Missing atoms parameter".to_string()))?;
            
            // Extract optional edges array
            let edges_value = arguments.get("edges").unwrap_or(&json!(null));
            
            // Call handler
            rpc::handle_publish_atoms(state, Some(json!({
                "agent_id": agent_id,
                "signature": signature,
                "atoms": atoms_value,
                "edges": edges_value,
            }))).await
        },
        "search_atoms" => {
            // Call handler with flat arguments
            rpc::handle_search_atoms(state, Some(arguments.clone())).await
        },
        "query_cluster" => {
            let vector = arguments
                .get("vector")
                .and_then(|v| v.as_array())
                .ok_or_else(|| MoteError::Validation("Missing vector parameter".to_string()))?
                .iter()
                .map(|v| {
                    v.as_f64()
                        .ok_or_else(|| MoteError::Validation("vector values must be numbers".to_string()))
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            
            let radius = arguments.get("radius")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| MoteError::Validation("Missing radius parameter".to_string()))?;
            
            // Call handler
            rpc::handle_query_cluster(state, Some(json!({
                "vector": vector,
                "radius": radius,
            }))).await
        },
        "claim_direction" => {
            // Extract parameters
            let agent_id = arguments.get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing agent_id parameter".to_string()))?;
            
            let signature = arguments.get("signature")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing signature parameter".to_string()))?;
            
            let hypothesis = arguments.get("hypothesis")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing hypothesis parameter".to_string()))?;
            
            let conditions = arguments.get("conditions")
                .and_then(|v| v.as_object())
                .ok_or_else(|| MoteError::Validation("Missing conditions parameter".to_string()))?;
            
            let domain = arguments.get("domain")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing domain parameter".to_string()))?;
            
            // Call handler
            rpc::handle_claim_direction(state, Some(json!({
                "agent_id": agent_id,
                "signature": signature,
                "hypothesis": hypothesis,
                "conditions": conditions,
                "domain": domain,
            }))).await
        },
        "retract_atom" => {
            // Extract parameters
            let agent_id = arguments.get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing agent_id parameter".to_string()))?;
            
            let signature = arguments.get("signature")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing signature parameter".to_string()))?;
            
            let atom_id = arguments.get("atom_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing atom_id parameter".to_string()))?;
            
            let reason = arguments.get("reason")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing reason parameter".to_string()))?;
            
            // Call handler
            rpc::handle_retract_atom(state, Some(json!({
                "agent_id": agent_id,
                "signature": signature,
                "atom_id": atom_id,
                "reason": reason,
            }))).await
        },
        "get_suggestions" => {
            let domain = arguments.get("domain").cloned().unwrap_or(Value::Null);
            let limit = arguments.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);
            
            // Call handler
            rpc::handle_get_suggestions(state, Some(json!({
                "domain": domain,
                "limit": limit,
            }))).await
        },
        "get_field_map" => {
            let domain = arguments
                .get("domain")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            
            // Call handler
            rpc::handle_get_field_map(state, Some(json!({
                "domain": domain,
            }))).await
        },
        _ => Err(MoteError::Validation(format!("Unknown tool: {}", tool_name))),
    }
}
