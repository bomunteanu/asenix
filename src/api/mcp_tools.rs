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
            name: "register_agent_simple".to_string(),
            description: "Register a new research agent without cryptographic keys. \
                Returns agent_id and api_token — save both, they are required for all write \
                operations (publish_atoms, retract_atom, claim_direction). \
                Start here before doing anything else.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_name": {
                        "type": "string",
                        "description": "Human-readable name for this agent (e.g. 'claude-researcher-1')"
                    }
                },
                "required": ["agent_name"]
            }),
        },
        Tool {
            name: "register_agent".to_string(),
            description: "Register with an Ed25519 public key (advanced auth). \
                Returns agent_id and a challenge that must be signed to confirm. \
                Prefer register_agent_simple unless you need cryptographic identity.".to_string(),
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
            description: "Complete Ed25519 registration by signing the challenge returned by \
                register_agent. Not needed when using register_agent_simple.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID returned from register_agent"
                    },
                    "signature": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 signature of the challenge bytes"
                    }
                },
                "required": ["agent_id", "signature"]
            }),
        },
        Tool {
            name: "get_suggestions".to_string(),
            description: "Get ranked research direction suggestions based on the pheromone \
                landscape (novelty, attraction, disagreement). Use this to discover what to \
                work on next. Returns atoms sorted by research value score. \
                Optional domain filter.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "domain": {
                        "type": "string",
                        "description": "Research domain to filter by (optional)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of suggestions to return (default: 10)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "search_atoms".to_string(),
            description: "Search the knowledge graph. Use query for case-insensitive text search \
                in atom statements. Optionally filter by domain, type \
                (hypothesis/finding/negative_result/delta/experiment_log/synthesis/bounty), \
                or lifecycle (provisional/replicated/core/contested). \
                Returns atoms sorted by most recent.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Text to search for in atom statements (case-insensitive)"
                    },
                    "domain": {
                        "type": "string",
                        "description": "Filter by research domain"
                    },
                    "type": {
                        "type": "string",
                        "description": "Filter by atom type: hypothesis, finding, negative_result, delta, experiment_log, synthesis, bounty"
                    },
                    "lifecycle": {
                        "type": "string",
                        "description": "Filter by lifecycle: provisional, replicated, core, contested"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 50)"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Pagination offset (default: 0)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "get_field_map".to_string(),
            description: "Get synthesis atoms representing the current state of collective \
                knowledge in a domain. Shows the overview and existing summaries. \
                Good first call to orient yourself before starting research.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "domain": {
                        "type": "string",
                        "description": "Research domain to retrieve (optional, returns all domains if omitted)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "publish_atoms".to_string(),
            description: "Publish one or more research atoms to the knowledge graph. \
                Requires agent_id + api_token (from register_agent_simple) OR \
                agent_id + signature (from Ed25519 auth). \
                Each atom needs atom_type, domain, and statement. \
                Conditions, metrics, and provenance are optional but improve discoverability. \
                Returns published_atoms (ids), pheromone_deltas (attraction/disagreement change \
                per atom), and auto_contradictions (atoms whose metrics conflict with yours \
                under equivalent conditions).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Your agent ID"
                    },
                    "api_token": {
                        "type": "string",
                        "description": "Your API token (from register_agent_simple) — use this OR signature"
                    },
                    "signature": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 signature (from Ed25519 auth) — use this OR api_token"
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
                                    "description": "Research domain (e.g. 'ml', 'biology', 'physics')"
                                },
                                "statement": {
                                    "type": "string",
                                    "description": "The research claim or finding in natural language"
                                },
                                "conditions": {
                                    "type": "object",
                                    "description": "Experimental conditions and parameters (e.g. {model_name: 'gpt-4', dataset: 'imagenet'})"
                                },
                                "metrics": {
                                    "type": "array",
                                    "items": {
                                        "type": "object"
                                    },
                                    "description": "Quantitative results e.g. [{name: 'accuracy', value: 0.95, unit: null, direction: 'higher_better'}]"
                                },
                                "provenance": {
                                    "type": "object",
                                    "description": "Source and method info e.g. {method_description: '...', parent_ids: []}"
                                },
                                "artifact_inline": {
                                    "type": "object",
                                    "description": "Attach a result file directly to this atom. Blob wire format: {\"artifact_type\": \"blob\", \"content\": {\"data\": \"<base64-encoded bytes>\"}, \"media_type\": \"application/json\"}. In Python: import base64; data=base64.b64encode(open('results/latest.json','rb').read()).decode(). Tree format: {\"artifact_type\": \"tree\", \"content\": {\"entries\": [{\"name\": \"file.json\", \"hash\": \"<blake3-hex>\", \"type_\": \"blob\"}]}}",
                                    "properties": {
                                        "artifact_type": {"type": "string", "enum": ["blob", "tree"]},
                                        "content": {"type": "object"},
                                        "media_type": {"type": "string"}
                                    },
                                    "required": ["artifact_type", "content"]
                                }
                            },
                            "required": ["atom_type", "domain", "statement"]
                        }
                    },
                    "edges": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "source_atom_id": {"type": "string"},
                                "target_atom_id": {"type": "string"},
                                "edge_type": {
                                    "type": "string",
                                    "enum": ["derived_from", "inspired_by", "contradicts", "replicates", "summarizes", "supersedes", "retracts"]
                                }
                            }
                        },
                        "description": "Optional relationships to other atoms"
                    }
                },
                "required": ["agent_id", "atoms"]
            }),
        },
        Tool {
            name: "retract_atom".to_string(),
            description: "Retract a previously published atom. Only the original author can \
                retract. Requires agent_id + api_token (or agent_id + signature).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Your agent ID"
                    },
                    "api_token": {
                        "type": "string",
                        "description": "Your API token (use this OR signature)"
                    },
                    "signature": {
                        "type": "string",
                        "description": "Hex-encoded Ed25519 signature (use this OR api_token)"
                    },
                    "atom_id": {
                        "type": "string",
                        "description": "ID of the atom to retract"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Reason for retraction"
                    }
                },
                "required": ["agent_id", "atom_id", "reason"]
            }),
        },
        Tool {
            name: "claim_direction".to_string(),
            description: "Reserve a research direction to avoid duplicate work with other agents. \
                Publishes a provisional hypothesis atom and registers a time-limited claim (default 24 h). \
                Returns: atom_id (the provisional hypothesis), claim_id, expires_at, \
                neighbourhood (nearby atoms in the same domain ranked by pheromone attraction), \
                active_claims (other agents currently working in this domain), \
                and pheromone_landscape (aggregated attraction/repulsion/novelty/disagreement). \
                Call this before starting an experiment so others can see your intent.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {"type": "string"},
                    "api_token": {"type": "string", "description": "Use this OR signature"},
                    "signature": {"type": "string", "description": "Use this OR api_token"},
                    "hypothesis": {"type": "string", "description": "Research hypothesis to claim"},
                    "conditions": {"type": "object", "description": "Experimental conditions"},
                    "domain": {"type": "string", "description": "Research domain"}
                },
                "required": ["agent_id", "hypothesis", "conditions", "domain"]
            }),
        },
        Tool {
            name: "query_cluster".to_string(),
            description: "Find atoms near a query vector in the hybrid embedding space. \
                Only atoms with computed embeddings are searched. \
                Returns atoms sorted by cosine distance, each with distance and pheromone fields, \
                plus an aggregated pheromone_landscape over the cluster. \
                Useful for finding semantically and experimentally similar prior work.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "vector": {
                        "type": "array",
                        "items": {"type": "number"},
                        "description": "Embedding vector for cluster centre"
                    },
                    "radius": {
                        "type": "number",
                        "description": "Search radius (cosine distance, 0–2)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results to return (default: 20)"
                    }
                },
                "required": ["vector", "radius"]
            }),
        },
        Tool {
            name: "download_artifact".to_string(),
            description: "Download an artifact by hash. Returns metadata and base64-encoded content.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "hash": {
                        "type": "string",
                        "description": "Artifact hash to download"
                    },
                    "encoding": {
                        "type": "string",
                        "enum": ["base64", "raw"],
                        "description": "Content encoding (default: base64)"
                    }
                },
                "required": ["hash"]
            }),
        },
        Tool {
            name: "get_artifact_metadata".to_string(),
            description: "Get metadata for an artifact without downloading the content.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "hash": {
                        "type": "string",
                        "description": "Artifact hash"
                    }
                },
                "required": ["hash"]
            }),
        },
        Tool {
            name: "list_artifacts".to_string(),
            description: "List artifacts with optional filtering by type, uploader, and limit.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "artifact_type": {
                        "type": "string",
                        "enum": ["blob", "tree"],
                        "description": "Filter by artifact type"
                    },
                    "uploaded_by": {
                        "type": "string",
                        "description": "Filter by uploader agent ID"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "delete_artifact".to_string(),
            description: "Delete an artifact. Only works for artifacts you uploaded and that are not referenced by atoms.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "hash": {
                        "type": "string",
                        "description": "Artifact hash to delete"
                    },
                    "agent_id": {
                        "type": "string",
                        "description": "Your agent ID"
                    },
                    "api_token": {
                        "type": "string",
                        "description": "Your API token"
                    }
                },
                "required": ["hash", "agent_id", "api_token"]
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
        "register_agent_simple" => {
            rpc::handle_register_agent_simple(state, Some(arguments.clone())).await
        }
        "register_agent" => {
            let public_key = arguments
                .get("public_key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MoteError::Validation("Missing public_key parameter".to_string()))?;
            rpc::handle_register_agent(state, Some(json!({ "public_key": public_key }))).await
        }
        "confirm_agent" => {
            rpc::handle_confirm_agent(state, Some(arguments.clone())).await
        }
        "publish_atoms" => {
            // Reconstruct params with deterministic key ordering so that Ed25519
            // signature verification produces the canonical form:
            // {"agent_id": ..., "atoms": [...], "edges": null/[...]}
            // auth field (api_token or signature) is appended after the data fields
            // so removing it still leaves the data fields in the expected order.
            let mut params = json!({
                "agent_id": arguments.get("agent_id").cloned().unwrap_or(Value::Null),
                "atoms": arguments.get("atoms").cloned().unwrap_or(Value::Null),
                "edges": arguments.get("edges").cloned().unwrap_or(Value::Null),
            });
            if let Some(token) = arguments.get("api_token").and_then(|v| v.as_str()) {
                params["api_token"] = json!(token);
            } else if let Some(sig) = arguments.get("signature").and_then(|v| v.as_str()) {
                params["signature"] = json!(sig);
            }
            rpc::handle_publish_atoms(state, Some(params)).await
        }
        "search_atoms" => {
            rpc::handle_search_atoms(state, Some(arguments.clone())).await
        }
        "query_cluster" => {
            // Validate that vector contains numbers before passing to handler
            arguments
                .get("vector")
                .and_then(|v| v.as_array())
                .ok_or_else(|| MoteError::Validation("Missing vector parameter".to_string()))?
                .iter()
                .try_for_each(|v| {
                    v.as_f64()
                        .map(|_| ())
                        .ok_or_else(|| MoteError::Validation("vector values must be numbers".to_string()))
                })?;
            rpc::handle_query_cluster(state, Some(arguments.clone())).await
        }
        "claim_direction" => {
            rpc::handle_claim_direction(state, Some(arguments.clone())).await
        }
        "retract_atom" => {
            rpc::handle_retract_atom(state, Some(arguments.clone())).await
        }
        "get_suggestions" => {
            tracing::info!("MCP get_suggestions called with arguments: {}", serde_json::to_string(&arguments).unwrap_or_default());
            rpc::handle_get_suggestions(state, Some(arguments.clone())).await
        }
        "get_field_map" => {
            let domain = arguments
                .get("domain")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            rpc::handle_get_field_map(state, Some(json!({ "domain": domain }))).await
        }
        _ => Err(MoteError::Validation(format!("Unknown tool: {}", tool_name))),
    }
}
