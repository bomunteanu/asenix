//! Unit tests for MCP tools functionality

use mote::api::mcp_tools::get_all_tools;
use serde_json::json;

#[tokio::test]
async fn test_get_all_tools() {
    let result = get_all_tools();
    
    // Should have 9 tools
    assert_eq!(result.tools.len(), 9);
    
    // Check specific tools exist
    let tool_names: Vec<String> = result.tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"register_agent".to_string()));
    assert!(tool_names.contains(&"confirm_agent".to_string()));
    assert!(tool_names.contains(&"publish_atoms".to_string()));
    assert!(tool_names.contains(&"search_atoms".to_string()));
    assert!(tool_names.contains(&"query_cluster".to_string()));
    assert!(tool_names.contains(&"claim_direction".to_string()));
    assert!(tool_names.contains(&"retract_atom".to_string()));
    assert!(tool_names.contains(&"get_suggestions".to_string()));
    assert!(tool_names.contains(&"get_field_map".to_string()));
}

#[tokio::test]
async fn test_tool_schemas() {
    let result = get_all_tools();
    
    // Test register_agent schema
    let register_agent = result.tools.iter().find(|t| t.name == "register_agent").unwrap();
    assert!(register_agent.description.contains("Ed25519 public key"));
    
    let schema = &register_agent.input_schema;
    assert_eq!(schema.get("type").unwrap().as_str(), Some("object"));
    assert!(schema.get("properties").unwrap().get("public_key").is_some());
    assert!(schema.get("required").unwrap().as_array().unwrap().contains(&json!("public_key")));
    
    // Test publish_atoms schema
    let publish_atoms = result.tools.iter().find(|t| t.name == "publish_atoms").unwrap();
    assert!(publish_atoms.description.contains("knowledge graph"));
    
    let schema = &publish_atoms.input_schema;
    let properties = schema.get("properties").unwrap().as_object().unwrap();
    
    // Check required fields
    assert!(properties.contains_key("agent_id"));
    assert!(properties.contains_key("signature"));
    assert!(properties.contains_key("atoms"));
    
    // Check atoms array structure
    let atoms_schema = properties.get("atoms").unwrap();
    assert_eq!(atoms_schema.get("type").unwrap().as_str(), Some("array"));
    let items = atoms_schema.get("items").unwrap();
    let item_props = items.get("properties").unwrap().as_object().unwrap();
    
    assert!(item_props.contains_key("atom_type"));
    assert!(item_props.contains_key("domain"));
    assert!(item_props.contains_key("statement"));
    assert!(item_props.contains_key("conditions"));
    assert!(item_props.contains_key("provenance"));
    assert!(item_props.contains_key("artifact_tree_hash"));
    
    // Check atom type enum
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
        assert!(tool.description.len() > 10); // Reasonable minimum length
    }
    
    // Check specific descriptions contain key information
    let confirm_agent = result.tools.iter().find(|t| t.name == "confirm_agent").unwrap();
    assert!(confirm_agent.description.contains("authentication challenge"));
    
    let search_atoms = result.tools.iter().find(|t| t.name == "search_atoms").unwrap();
    assert!(search_atoms.description.contains("filters"));
    assert!(search_atoms.description.contains("knowledge graph"));
    
    let query_cluster = result.tools.iter().find(|t| t.name == "query_cluster").unwrap();
    assert!(query_cluster.description.contains("embedding space"));
    assert!(query_cluster.description.contains("pheromone landscape"));
}

#[tokio::test]
async fn test_required_parameters() {
    let result = get_all_tools();
    
    // Test register_agent required parameters
    let register_agent = result.tools.iter().find(|t| t.name == "register_agent").unwrap();
    let required = register_agent.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("public_key")));
    
    // Test confirm_agent required parameters
    let confirm_agent = result.tools.iter().find(|t| t.name == "confirm_agent").unwrap();
    let required = confirm_agent.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("agent_id")));
    assert!(required.contains(&json!("signature")));
    
    // Test publish_atoms required parameters
    let publish_atoms = result.tools.iter().find(|t| t.name == "publish_atoms").unwrap();
    let required = publish_atoms.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 3);
    assert!(required.contains(&json!("agent_id")));
    assert!(required.contains(&json!("signature")));
    assert!(required.contains(&json!("atoms")));
    
    // Test search_atoms has no required parameters
    let search_atoms = result.tools.iter().find(|t| t.name == "search_atoms").unwrap();
    let required = search_atoms.input_schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 0);
}

#[tokio::test]
async fn test_optional_parameters() {
    let result = get_all_tools();
    
    // Test publish_atoms optional parameters
    let publish_atoms = result.tools.iter().find(|t| t.name == "publish_atoms").unwrap();
    let properties = publish_atoms.input_schema.get("properties").unwrap().as_object().unwrap();
    
    // metrics in atoms should be optional
    let atoms_schema = properties.get("atoms").unwrap().get("items").unwrap();
    let atom_props = atoms_schema.get("properties").unwrap().as_object().unwrap();
    let metrics_schema = atom_props.get("metrics").unwrap();
    assert!(metrics_schema.get("nullable").unwrap().as_bool().unwrap());
    
    // Test get_suggestions optional parameters
    let get_suggestions = result.tools.iter().find(|t| t.name == "get_suggestions").unwrap();
    let properties = get_suggestions.input_schema.get("properties").unwrap().as_object().unwrap();
    
    assert!(properties.contains_key("domain"));
    assert!(properties.contains_key("limit"));
}
