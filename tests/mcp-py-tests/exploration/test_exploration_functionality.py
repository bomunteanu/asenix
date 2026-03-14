#!/usr/bin/env python3
"""
Quick test to verify the new exploration functionality works
"""

import sys
import os
import json
import asyncio

# Add the mote_client to path
sys.path.append(os.path.join(os.path.dirname(__file__), '..', '..', '..', 'asenix_client'))

from asenix_mcp_client import AsenixMCPClient

def extract_content(response):
    """Extract content from MCP response format"""
    content = response.get("result", {}).get("content", [])
    if content and len(content) > 0:
        content_text = content[0].get("text", "")
        return json.loads(content_text)
    return {}

async def test_exploration_mode():
    """Test the new exploration mode functionality"""
    
    print("🧪 Testing exploration mode functionality...")
    
    # Connect to Mote server
    server_url = "http://localhost:3000"
    
    client = AsenixMCPClient(server_url)
    client.initialize()
    
    # Register a test agent
    print("📝 Registering test agent...")
    register_response = client.call_tool("register_agent_simple", {
        "agent_name": "exploration-test-agent"
    })
    
    # Extract the actual content from MCP response
    agent_data = extract_content(register_response)
    agent_id = agent_data.get("agent_id")
    api_token = agent_data.get("api_token")
    
    if not agent_id or not api_token:
        print("❌ Failed to get agent_id or api_token from register response")
        print(f"Register response: {json.dumps(register_response, indent=2)}")
        return
    print(f"✅ Registered agent: {agent_id}")
    
    # Test 1: get_suggestions WITHOUT exploration (backward compatibility)
    print("\n🔍 Testing get_suggestions WITHOUT exploration...")
    response = client.call_tool("get_suggestions", {
        "agent_id": agent_id,
        "api_token": api_token,
        "limit": 5
    })
    
    response_data = extract_content(response)
    assert response_data["strategy"] == "pheromone_attraction"
    print(f"✅ Backward compatibility works: strategy = {response_data['strategy']}")
    
    # Test 2: get_suggestions WITH exploration mode
    print("\n🔍 Testing get_suggestions WITH exploration mode...")
    response = client.call_tool("get_suggestions", {
        "agent_id": agent_id,
        "api_token": api_token,
        "limit": 5,
        "include_exploration": True
    })
    
    response_data = extract_content(response)
    print(f"📝 Exploration response: {json.dumps(response_data, indent=2)}")
    assert response_data["strategy"] == "pheromone_attraction_plus_exploration"
    print(f"✅ Exploration mode works: strategy = {response_data['strategy']}")
    
    suggestions = response_data["suggestions"]
    pheromone_suggestions = [s for s in suggestions if s.get("source") == "pheromone"]
    exploration_suggestions = [s for s in suggestions if s.get("source") == "exploration"]
    
    print(f"📊 Found {len(pheromone_suggestions)} pheromone suggestions")
    print(f"📊 Found {len(exploration_suggestions)} exploration suggestions")
    
    # Check that exploration suggestions have the expected fields
    for suggestion in exploration_suggestions:
        assert "novelty" in suggestion
        assert "atom_count" in suggestion
        assert suggestion["novelty"] > 0.5  # Should be above threshold
        print(f"✅ Exploration suggestion has novelty={suggestion['novelty']:.3f}, atom_count={suggestion['atom_count']}")
    
    # Test 3: Test domain filtering with exploration
    print("\n🔍 Testing domain filtering with exploration...")
    response = client.call_tool("get_suggestions", {
        "agent_id": agent_id,
        "api_token": api_token,
        "limit": 3,
        "domain": "test",  # Filter to test domain
        "include_exploration": True
    })
    
    response_data = extract_content(response)
    assert response_data["strategy"] == "pheromone_attraction_plus_exploration"
    print(f"✅ Domain filtering works with exploration mode")
    
    print("\n🎉 All exploration functionality tests passed!")

async def main():
    """Main entry point"""
    try:
        await test_exploration_mode()
    except Exception as e:
        print(f"❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())
