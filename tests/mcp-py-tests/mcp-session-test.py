#!/usr/bin/env python3
"""
Comprehensive MCP vs RPC Endpoint Test

This script tests both endpoints to validate:
1. MCP session protocol works end-to-end
2. RPC endpoint continues to work as before
3. Error handling is consistent between endpoints
"""

import requests
import json
import time
from typing import Dict, Any

# Configuration
BASE_URL = "http://localhost:3000"
MCP_URL = f"{BASE_URL}/mcp"
RPC_URL = f"{BASE_URL}/rpc"

def test_mcp_session_protocol():
    """Test the complete MCP session lifecycle"""
    print("🔍 Testing MCP Session Protocol...")
    
    # Step 1: Initialize session
    print("  1. Initializing session...")
    init_response = requests.post(MCP_URL, headers={
        "Content-Type": "application/json",
        "Origin": "http://localhost:3000",
        "Accept": "application/json, text/event-stream"
    }, json={
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        },
        "id": 1
    })
    
    if init_response.status_code != 200:
        print(f"    ❌ Initialize failed: {init_response.status_code}")
        return False
    
    init_data = init_response.json()
    session_id = init_response.headers.get("mcp-session-id")
    if not session_id:
        print(f"    ❌ No MCP-Session-Id header in response: {init_data}")
        return False
    
    print(f"    ✅ Session initialized: {session_id[:16]}...")
    
    # Step 2: Send notifications/initialized
    print("  2. Sending notifications/initialized...")
    notif_response = requests.post(MCP_URL, headers={
        "Content-Type": "application/json",
        "Origin": "http://localhost:3000",
        "Accept": "application/json, text/event-stream",
        "mcp-session-id": session_id
    }, json={
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    })
    
    if notif_response.status_code not in (200, 202):
        print(f"    ❌ Notifications/initialized failed: {notif_response.status_code}")
        return False
    
    print("    ✅ Session marked as initialized")
    
    # Step 3: Call tools/list (should work now)
    print("  3. Calling tools/list...")
    tools_response = requests.post(MCP_URL, headers={
        "Content-Type": "application/json",
        "Origin": "http://localhost:3000",
        "Accept": "application/json, text/event-stream",
        "mcp-session-id": session_id
    }, json={
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {},
        "id": 3
    })
    
    if tools_response.status_code != 200:
        print(f"    ❌ Tools/list failed: {tools_response.status_code}")
        return False
    
    tools_data = tools_response.json()
    if "error" in tools_data:
        print(f"    ❌ Tools/list returned error: {tools_data['error']}")
        return False
    
    print("    ✅ Tools listed successfully")
    
    # Step 4: Test error handling (invalid method)
    print("  4. Testing error handling...")
    error_response = requests.post(MCP_URL, headers={
        "Content-Type": "application/json",
        "Origin": "http://localhost:3000",
        "Accept": "application/json, text/event-stream",
        "mcp-session-id": session_id
    }, json={
        "jsonrpc": "2.0",
        "method": "invalid_method",
        "params": {},
        "id": 4
    })
    
    if error_response.status_code != 200:
        print(f"    ❌ Error test failed: {error_response.status_code}")
        return False
    
    error_data = error_response.json()
    if "error" not in error_data:
        print("    ❌ Expected error response")
        return False
    
    error_code = error_data["error"].get("code")
    if error_code != -32601:  # Method not found
        print(f"    ❌ Wrong error code: {error_code}")
        return False
    
    print("    ✅ Error handling correct (-32601)")
    
    # Step 5: Terminate session
    print("  5. Terminating session...")
    delete_response = requests.delete(f"{BASE_URL}/mcp", headers={
        "mcp-session-id": session_id
    })
    
    if delete_response.status_code != 200:
        print(f"    ❌ Session termination failed: {delete_response.status_code}")
        return False
    
    print("    ✅ Session terminated")
    return True

def test_rpc_endpoint():
    """Test that RPC endpoint still works"""
    print("\n🔍 Testing RPC Endpoint...")
    
    # Test health check via RPC
    response = requests.post(RPC_URL, json={
        "jsonrpc": "2.0",
        "method": "search_atoms",
        "params": {"limit": 5},
        "id": 1
    })
    
    if response.status_code != 200:
        print(f"    ❌ RPC call failed: {response.status_code}")
        return False
    
    data = response.json()
    # Check if error is null (success) or contains error
    if data.get("error") is not None:
        print(f"    ❌ RPC returned error: {data['error']}")
        return False
    
    print("    ✅ RPC endpoint working")
    return True

def test_error_consistency():
    """Test that error codes are consistent between endpoints"""
    print("\n🔍 Testing Error Consistency...")
    
    # Test validation error on MCP (missing session header)
    mcp_error_response = requests.post(MCP_URL, headers={
        "Content-Type": "application/json",
        "Origin": "http://localhost:3000",
        "Accept": "application/json, text/event-stream"
        # Missing mcp-session-id header
    }, json={
        "jsonrpc": "2.0",
        "method": "tools/list",
        "params": {},
        "id": 1
    })
    
    mcp_error_code = None
    if mcp_error_response.status_code == 200:
        mcp_data = mcp_error_response.json()
        if "error" in mcp_data:
            mcp_error_code = mcp_data["error"].get("code")
    
    # Test validation error on RPC (missing required params)
    rpc_error_response = requests.post(RPC_URL, json={
        "jsonrpc": "2.0",
        "method": "register_agent",
        "params": {},  # Missing public_key
        "id": 1
    })
    
    rpc_error_code = None
    if rpc_error_response.status_code == 200:
        rpc_data = rpc_error_response.json()
        if "error" in rpc_data:
            rpc_error_code = rpc_data["error"].get("code")
    
    print(f"    MCP validation error code: {mcp_error_code}")
    print(f"    RPC validation error code: {rpc_error_code}")
    
    # Both should return -32602 for validation errors
    if mcp_error_code == rpc_error_code == -32602:
        print("    ✅ Error codes consistent (-32602)")
        return True
    else:
        print("    ❌ Error codes inconsistent")
        return False

def main():
    print("🚀 Comprehensive MCP vs RPC Endpoint Test")
    print("=" * 50)
    
    # Test server health first
    try:
        health_response = requests.get(f"{BASE_URL}/health", timeout=5)
        if health_response.status_code != 200:
            print("❌ Server not healthy")
            return
        print("✅ Server healthy")
    except requests.exceptions.RequestException as e:
        print(f"❌ Server health check failed: {e}")
        return
    
    # Run tests
    results = []
    results.append(("MCP Session Protocol", test_mcp_session_protocol()))
    results.append(("RPC Endpoint", test_rpc_endpoint()))
    results.append(("Error Consistency", test_error_consistency()))
    
    # Summary
    print("\n" + "=" * 50)
    print("📊 TEST SUMMARY")
    print("=" * 50)
    
    all_passed = True
    for test_name, passed in results:
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{test_name:} {status}")
        if not passed:
            all_passed = False
    
    print("=" * 50)
    if all_passed:
        print("🎉 ALL TESTS PASSED! MCP endpoint is working correctly.")
    else:
        print("⚠️  Some tests failed. Check the output above.")
    
    return all_passed

if __name__ == "__main__":
    main()
