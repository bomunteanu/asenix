#!/usr/bin/env python3
"""
Test the RPC endpoint directly to see if exploration mode works
"""

import requests
import json

def test_rpc_direct():
    """Test the RPC endpoint directly"""
    
    print("🧪 Testing RPC endpoint directly...")
    
    server_url = "http://localhost:3000"
    
    # Register a test agent
    print("📝 Registering test agent...")
    register_response = requests.post(f"{server_url}/rpc", json={
        "jsonrpc": "2.0",
        "id": 1,
        "method": "register_agent_simple",
        "params": {
            "agent_name": "rpc-exploration-test"
        }
    })
    
    if register_response.status_code != 200:
        print(f"❌ Registration failed: {register_response.status_code}")
        return
    
    register_data = register_response.json()
    agent_id = register_data.get("result", {}).get("agent_id")
    api_token = register_data.get("result", {}).get("api_token")
    
    if not agent_id or not api_token:
        print("❌ Failed to get agent_id or api_token")
        print(f"Response: {json.dumps(register_data, indent=2)}")
        return
    
    print(f"✅ Registered agent: {agent_id}")
    
    # Test get_suggestions WITH exploration mode via RPC
    print("\n🔍 Testing get_suggestions WITH exploration mode via RPC...")
    response = requests.post(f"{server_url}/rpc", json={
        "jsonrpc": "2.0",
        "id": 2,
        "method": "get_suggestions",
        "params": {
            "agent_id": agent_id,
            "api_token": api_token,
            "limit": 5,
            "include_exploration": True
        }
    })
    
    if response.status_code != 200:
        print(f"❌ Request failed: {response.status_code}")
        print(f"Response: {response.text}")
        return
    
    response_data = response.json()
    result = response_data.get("result", {})
    
    print(f"📝 Strategy: {result.get('strategy', 'MISSING')}")
    print(f"📝 Description: {result.get('description', 'MISSING')}")
    
    # Show debug info
    debug_info = result.get("debug", {})
    if debug_info:
        print(f"🐛 Debug info: {json.dumps(debug_info, indent=2)}")
    
    # Check if exploration suggestions are present
    suggestions = result.get("suggestions", [])
    exploration_suggestions = [s for s in suggestions if s.get("source") == "exploration"]
    
    print(f"📊 Total suggestions: {len(suggestions)}")
    print(f"📊 Exploration suggestions: {len(exploration_suggestions)}")
    
    if result.get("strategy") == "pheromone_attraction_plus_exploration":
        print("✅ Exploration mode is working via RPC!")
    else:
        print("❌ Exploration mode is NOT working via RPC")
        print("❌ This suggests there's an issue with the implementation itself")

if __name__ == "__main__":
    test_rpc_direct()
