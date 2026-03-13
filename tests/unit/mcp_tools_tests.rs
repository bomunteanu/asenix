//! Unit tests for MCP tools functionality

use mote::api::mcp_tools::get_all_tools;
use serde_json::json;

#[tokio::test]
async fn test_get_all_tools() {
    let result = get_all_tools();

    // register_agent_simple added + 4 artifact tools = 14 tools total
    assert_eq!(result.tools.len(), 14);

    let tool_names: Vec<String> = result.tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"register_agent_simple".to_string()));
    assert!(tool_names.contains(&"register_agent".to_string()));
    assert!(tool_names.contains(&"confirm_agent".to_string()));
    assert!(tool_names.contains(&"publish_atoms".to_string()));
    assert!(tool_names.contains(&"search_atoms".to_string()));
    assert!(tool_names.contains(&"query_cluster".to_string()));
    assert!(tool_names.contains(&"claim_direction".to_string()));
    assert!(tool_names.contains(&"retract_atom".to_string()));
    assert!(tool_names.contains(&"get_suggestions".to_string()));
    assert!(tool_names.contains(&"get_field_map".to_string()));
    // New artifact tools
    assert!(tool_names.contains(&"download_artifact".to_string()));
    assert!(tool_names.contains(&"get_artifact_metadata".to_string()));
    assert!(tool_names.contains(&"list_artifacts".to_string()));
    assert!(tool_names.contains(&"delete_artifact".to_string()));
}

#[tokio::test]
async fn test_tool_schemas() {
    let result = get_all_tools();

    // register_agent_simple schema
    let simple = result.tools.iter().find(|t| t.name == "register_agent_simple").unwrap();
    assert!(simple.input_schema.get("properties").unwrap().get("agent_name").is_some());
    assert!(simple.input_schema.get("required").unwrap().as_array().unwrap().contains(&json!("agent_name")));

    // register_agent schema
    let register_agent = result.tools.iter().find(|t| t.name == "register_agent").unwrap();
    assert!(register_agent.description.contains("Ed25519 public key"));
    let schema = &register_agent.input_schema;
    assert_eq!(schema.get("type").unwrap().as_str(), Some("object"));
    assert!(schema.get("properties").unwrap().get("public_key").is_some());
    assert!(schema.get("required").unwrap().as_array().unwrap().contains(&json!("public_key")));

    // publish_atoms schema — accepts api_token or signature, not required
    let publish_atoms = result.tools.iter().find(|t| t.name == "publish_atoms").unwrap();
    assert!(publish_atoms.description.contains("knowledge graph"));
    let properties = publish_atoms.input_schema.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("agent_id"));
    assert!(properties.contains_key("api_token"));
    assert!(properties.contains_key("signature"));
    assert!(properties.contains_key("atoms"));

    // atoms array structure
    let atoms_schema = properties.get("atoms").unwrap();
    assert_eq!(atoms_schema.get("type").unwrap().as_str(), Some("array"));
    let item_props = atoms_schema.get("items").unwrap().get("properties").unwrap().as_object().unwrap();
    assert!(item_props.contains_key("atom_type"));
    assert!(item_props.contains_key("domain"));
    assert!(item_props.contains_key("statement"));
    assert!(item_props.contains_key("conditions"));
    assert!(item_props.contains_key("provenance"));

    // atom_type enum
    let atom_type_enum = item_props.get("atom_type").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(atom_type_enum.contains(&json!("hypothesis")));
    assert!(atom_type_enum.contains(&json!("finding")));
    assert!(atom_type_enum.contains(&json!("negative_result")));
}

#[tokio::test]
async fn test_tool_descriptions() {
    let result = get_all_tools();

    // All tools should have non-empty descriptions
    for tool in &result.tools {
        assert!(!tool.description.is_empty());
        assert!(tool.description.len() > 10);
    }

    // register_agent_simple is the starting point for AI agents
    let simple = result.tools.iter().find(|t| t.name == "register_agent_simple").unwrap();
    assert!(simple.description.contains("api_token"));

    // confirm_agent mentions signing the challenge
    let confirm_agent = result.tools.iter().find(|t| t.name == "confirm_agent").unwrap();
    assert!(confirm_agent.description.contains("challenge"));

    // search_atoms explains text search and the knowledge graph
    let search_atoms = result.tools.iter().find(|t| t.name == "search_atoms").unwrap();
    assert!(search_atoms.description.contains("knowledge graph") || search_atoms.description.contains("search"));

    // query_cluster mentions embedding space
    let query_cluster = result.tools.iter().find(|t| t.name == "query_cluster").unwrap();
    assert!(query_cluster.description.contains("embedding space"));
}

#[tokio::test]
async fn test_required_parameters() {
    let result = get_all_tools();

    // register_agent_simple requires agent_name
    let simple = result.tools.iter().find(|t| t.name == "register_agent_simple").unwrap();
    let required = simple.input_schema.get("required").unwrap().as_array().unwrap();
    assert!(required.contains(&json!("agent_name")));

    // register_agent requires public_key
    let register_agent = result.tools.iter().find(|t| t.name == "register_agent").unwrap();
    let required = register_agent.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("public_key")));

    // confirm_agent requires agent_id and signature
    let confirm_agent = result.tools.iter().find(|t| t.name == "confirm_agent").unwrap();
    let required = confirm_agent.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("agent_id")));
    assert!(required.contains(&json!("signature")));

    // publish_atoms requires only agent_id and atoms (api_token/signature are optional-but-one-required)
    let publish_atoms = result.tools.iter().find(|t| t.name == "publish_atoms").unwrap();
    let required = publish_atoms.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("agent_id")));
    assert!(required.contains(&json!("atoms")));

    // search_atoms has no required parameters
    let search_atoms = result.tools.iter().find(|t| t.name == "search_atoms").unwrap();
    let required = search_atoms.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 0);
}

#[tokio::test]
async fn test_optional_parameters() {
    let result = get_all_tools();

    // publish_atoms atoms items have optional metrics
    let publish_atoms = result.tools.iter().find(|t| t.name == "publish_atoms").unwrap();
    let properties = publish_atoms.input_schema.get("properties").unwrap().as_object().unwrap();
    let atoms_schema = properties.get("atoms").unwrap().get("items").unwrap();
    let atom_props = atoms_schema.get("properties").unwrap().as_object().unwrap();
    assert!(atom_props.contains_key("metrics"));

    // get_suggestions has optional domain and limit
    let get_suggestions = result.tools.iter().find(|t| t.name == "get_suggestions").unwrap();
    let properties = get_suggestions.input_schema.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("domain"));
    assert!(properties.contains_key("limit"));

    // search_atoms has optional query field for text search
    let search_atoms = result.tools.iter().find(|t| t.name == "search_atoms").unwrap();
    let properties = search_atoms.input_schema.get("properties").unwrap().as_object().unwrap();
    assert!(properties.contains_key("query"));
}
