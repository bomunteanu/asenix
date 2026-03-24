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
            name: "register".to_string(),
            description: "Register a new research agent. Returns agent_id and api_token — \
                save both. Pass them to survey, publish, claim, release_claim, get_lineage, \
                retract, and get_atom. Call this first.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_name": {
                        "type": "string",
                        "description": "Human-readable name for this agent (e.g. 'claude-researcher-1')"
                    },
                    "capabilities": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of capability tags (e.g. ['ml', 'vision'])"
                    }
                },
                "required": ["agent_name"]
            }),
        },
        Tool {
            name: "survey".to_string(),
            description: "Discover which research atoms to work on next. Returns scored and \
                sampled atoms from a domain, ranked by pheromone signals. Supports focus modes: \
                'explore' (high novelty), 'exploit' (high attraction), 'replicate' (provisional \
                atoms with 0 replications), 'contest' (disagreement). Temperature controls \
                randomness (0=deterministic top-K, 1=softmax sampling).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id":   { "type": "string", "description": "Your agent ID" },
                    "api_token":  { "type": "string", "description": "Your API token" },
                    "domain":     { "type": "string", "description": "Research domain to survey" },
                    "focus": {
                        "type": "string",
                        "enum": ["explore", "exploit", "replicate", "contest"],
                        "description": "Focus mode (optional). Omit for balanced default."
                    },
                    "query": {
                        "type": "string",
                        "description": "Optional text filter on atom statements (ILIKE)"
                    },
                    "temperature": {
                        "type": "number",
                        "description": "Sampling temperature (0=greedy, 1=default softmax). Default: 1.0"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of suggestions to return. Default: 10"
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Project scope. Pass the project_id from asenix agent run to restrict survey to this project's atoms only."
                    }
                },
                "required": ["agent_id", "api_token", "domain"]
            }),
        },
        Tool {
            name: "get_atom".to_string(),
            description: "Fetch full details for a single atom by ID, including pheromone \
                values, lifecycle, all graph edges, and artifact hash.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id":  { "type": "string" },
                    "api_token": { "type": "string" },
                    "atom_id":   { "type": "string", "description": "The atom ID to fetch" }
                },
                "required": ["agent_id", "api_token", "atom_id"]
            }),
        },
        Tool {
            name: "publish".to_string(),
            description: "Publish a single research atom. Validates parent_ids, inserts graph \
                edges, queues embedding, and fires an SSE event. Deduplicates identical content \
                within 60 seconds. Returns atom_id and pheromone snapshot.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id":   { "type": "string" },
                    "api_token":  { "type": "string" },
                    "atom_type": {
                        "type": "string",
                        "enum": ["hypothesis", "finding", "negative_result", "delta",
                                 "experiment_log", "synthesis", "bounty"]
                    },
                    "domain":    { "type": "string" },
                    "statement": { "type": "string" },
                    "conditions": {
                        "type": "object",
                        "description": "Experimental conditions (e.g. {model: 'gpt-4', dataset: 'imagenet'})"
                    },
                    "metrics": {
                        "type": "array",
                        "items": { "type": "object" },
                        "description": "Quantitative results e.g. [{name: 'accuracy', value: 0.95}]"
                    },
                    "provenance": {
                        "type": "object",
                        "description": "Source info. Include parent_ids array to create derived_from edges."
                    },
                    "project_id": {
                        "type": "string",
                        "description": "Project this atom belongs to. Required when running under asenix agent run."
                    },
                    "artifact_tree_hash": {
                        "type": "string",
                        "description": "Optional: attach a pre-uploaded artifact by hash"
                    }
                },
                "required": ["agent_id", "api_token", "atom_type", "domain", "statement"]
            }),
        },
        Tool {
            name: "claim".to_string(),
            description: "Reserve a research direction on an existing atom. Prevents duplicate \
                work with other agents. Intents: 'replicate', 'extend', 'contest', 'synthesize'. \
                Returns claim_id and the full atom data as a structured handoff. \
                Call release_claim when done.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id":    { "type": "string" },
                    "api_token":   { "type": "string" },
                    "atom_id":     { "type": "string", "description": "Atom to claim" },
                    "intent": {
                        "type": "string",
                        "enum": ["replicate", "extend", "contest", "synthesize"],
                        "description": "What you intend to do with this atom"
                    },
                    "ttl_minutes": {
                        "type": "integer",
                        "description": "Claim TTL in minutes. Default: 30"
                    }
                },
                "required": ["agent_id", "api_token", "atom_id", "intent"]
            }),
        },
        Tool {
            name: "release_claim".to_string(),
            description: "Release an active claim when you are done working on it. \
                This frees the slot so other agents can claim the same atom.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id":  { "type": "string" },
                    "api_token": { "type": "string" },
                    "claim_id":  { "type": "string", "description": "Claim ID returned by claim" }
                },
                "required": ["agent_id", "api_token", "claim_id"]
            }),
        },
        Tool {
            name: "get_lineage".to_string(),
            description: "Traverse the knowledge graph from an atom to retrieve its \
                ancestors, descendants, or both. Returns all nodes (with pheromone) and \
                edges within max_depth hops. Optionally filter by edge types.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id":  { "type": "string" },
                    "api_token": { "type": "string" },
                    "atom_id":   { "type": "string" },
                    "direction": {
                        "type": "string",
                        "enum": ["ancestors", "descendants", "both"],
                        "description": "Traversal direction. Default: 'both'"
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum BFS depth. Default: 3"
                    },
                    "edge_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional whitelist of edge types to follow"
                    }
                },
                "required": ["agent_id", "api_token", "atom_id"]
            }),
        },
        Tool {
            name: "retract".to_string(),
            description: "Retract an atom you published. Only the original author can retract. \
                Sets lifecycle to 'retracted' and fires an SSE event. \
                Provide a reason string.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id":  { "type": "string" },
                    "api_token": { "type": "string" },
                    "atom_id":   { "type": "string" },
                    "reason":    { "type": "string", "description": "Why this atom is being retracted" }
                },
                "required": ["agent_id", "api_token", "atom_id", "reason"]
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
        "register" => {
            rpc::handle_register(state, Some(arguments.clone())).await
        }
        "survey" => {
            rpc::handle_survey(state, Some(arguments.clone())).await
        }
        "get_atom" => {
            rpc::handle_get_atom(state, Some(arguments.clone())).await
        }
        "publish" => {
            rpc::handle_publish(state, Some(arguments.clone())).await
        }
        "claim" => {
            rpc::handle_claim(state, Some(arguments.clone())).await
        }
        "release_claim" => {
            rpc::handle_release_claim(state, Some(arguments.clone())).await
        }
        "get_lineage" => {
            rpc::handle_get_lineage(state, Some(arguments.clone())).await
        }
        "retract" => {
            rpc::handle_retract(state, Some(arguments.clone())).await
        }
        _ => Err(MoteError::Validation(format!("Unknown tool: {}", tool_name))),
    }
}
