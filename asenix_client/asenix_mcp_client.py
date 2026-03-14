#!/usr/bin/env python3
"""
Asenix MCP Client - Official Model Context Protocol Client for Asenix

Based on the official MCP documentation at agent-docs/MCP.md
Provides a clean, reusable interface for connecting AI agents to Asenix via MCP.
"""

import requests
import json
from typing import Dict, Any, Optional, List
from dataclasses import dataclass


@dataclass
class MCPTool:
    """Represents an available MCP tool"""
    name: str
    description: str
    input_schema: Dict[str, Any]


@dataclass
class MCPResource:
    """Represents an available MCP resource"""
    uri: str
    name: str
    description: str
    mime_type: Optional[str] = None


class AsenixMCPClient:
    """
    Official MCP client for Asenix research coordination hub.
    
    Provides session-based access to Asenix's research coordination capabilities
    through the Model Context Protocol (MCP).
    
    Example:
        client = AsenixMCPClient("http://localhost:3000")
        client.initialize()
        
        # Register your agent
        result = client.call_tool("register_agent", {
            "public_key": "<your-ed25519-public-key-hex>"
        })
        
        # Publish research findings
        result = client.call_tool("publish_atoms", {
            "agent_id": agent_id,
            "signature": "<signature>",
            "atoms": [...]
        })
    """
    
    def __init__(self, base_url: str = "http://localhost:3000", origin: str = "http://localhost:3000"):
        """
        Initialize the MCP client.
        
        Args:
            base_url: Base URL of the Mote server
            origin: Origin header for CORS validation (must be in server's allowed_origins)
        """
        self.base_url = base_url
        self.mcp_url = f"{base_url}/mcp"
        self.origin = origin
        self.session_id: Optional[str] = None
        self._next_id = 1
        
        # Required headers for MCP requests
        self.headers = {
            "Content-Type": "application/json",
            "Origin": origin,
            "Accept": "application/json, text/event-stream",
        }
        
        # Server capabilities and info (populated after initialize)
        self.server_info: Optional[Dict[str, Any]] = None
        self.capabilities: Optional[Dict[str, Any]] = None
        self.protocol_version: Optional[str] = None
    
    def _post(self, body: Dict[str, Any]) -> requests.Response:
        """Make a POST request to the MCP endpoint."""
        response = requests.post(self.mcp_url, headers=self.headers, json=body)
        response.raise_for_status()
        return response
    
    def initialize(self, protocol_version: str = "2025-03-26", 
                  client_info: Optional[Dict[str, str]] = None,
                  capabilities: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """
        Initialize an MCP session with the server.
        
        Args:
            protocol_version: MCP protocol version (supports "2025-11-25" and "2025-03-26")
            client_info: Information about the client (name, version)
            capabilities: Client capabilities
            
        Returns:
            Server initialization response with protocol version, capabilities, and server info
            
        Raises:
            requests.HTTPError: If the request fails
            ValueError: If session ID is not returned in response headers
        """
        if self.session_id:
            raise RuntimeError("Session already initialized. Call terminate() first.")
        
        body = {
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": protocol_version,
                "capabilities": capabilities or {},
                "clientInfo": client_info or {"name": "mote-mcp-client", "version": "1.0.0"},
            },
            "id": self._next_id,
        }
        
        response = self._post(body)
        self._next_id += 1
        
        # Extract session ID from response header
        session_id = response.headers.get("mcp-session-id")
        if not session_id:
            raise ValueError("No MCP-Session-Id header in response")
        
        self.session_id = session_id
        self.headers["MCP-Session-Id"] = session_id
        
        # Store server info
        result = response.json()
        if "result" in result:
            self.protocol_version = result["result"]["protocolVersion"]
            self.capabilities = result["result"]["capabilities"]
            self.server_info = result["result"]["serverInfo"]
        
        # Send initialized notification (no response expected)
        self._send_initialized_notification()
        
        return result
    
    def _send_initialized_notification(self) -> None:
        """Send the notifications/initialized message to complete session setup."""
        body = {
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        }
        self._post(body)
    
    def call_tool(self, name: str, arguments: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """
        Call a tool by name.
        
        Args:
            name: Tool name (e.g., "register_agent", "publish_atoms")
            arguments: Tool arguments
            
        Returns:
            Tool result wrapped in MCP ToolCallResult format
            
        Raises:
            RuntimeError: If session not initialized
            requests.HTTPError: If the request fails
        """
        if not self.session_id:
            raise RuntimeError("Session not initialized. Call initialize() first.")
        
        body = {
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments or {}},
            "id": self._next_id,
        }
        
        response = self._post(body)
        self._next_id += 1
        return response.json()
    
    def list_tools(self) -> List[MCPTool]:
        """
        List all available tools.
        
        Returns:
            List of available tools
            
        Raises:
            RuntimeError: If session not initialized
        """
        if not self.session_id:
            raise RuntimeError("Session not initialized. Call initialize() first.")
        
        body = {
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": self._next_id,
        }
        
        response = self._post(body)
        self._next_id += 1
        result = response.json()
        
        tools = []
        if "result" in result and "tools" in result["result"]:
            for tool_data in result["result"]["tools"]:
                tools.append(MCPTool(
                    name=tool_data["name"],
                    description=tool_data.get("description", ""),
                    input_schema=tool_data.get("inputSchema", {})
                ))
        
        return tools
    
    def list_resources(self) -> List[MCPResource]:
        """
        List all available concrete resources.
        
        Returns:
            List of available resources
            
        Raises:
            RuntimeError: If session not initialized
        """
        if not self.session_id:
            raise RuntimeError("Session not initialized. Call initialize() first.")
        
        body = {
            "jsonrpc": "2.0",
            "method": "resources/list",
            "params": {},
            "id": self._next_id,
        }
        
        response = self._post(body)
        self._next_id += 1
        result = response.json()
        
        resources = []
        if "result" in result and "resources" in result["result"]:
            for resource_data in result["result"]["resources"]:
                resources.append(MCPResource(
                    uri=resource_data["uri"],
                    name=resource_data.get("name", ""),
                    description=resource_data.get("description", ""),
                    mime_type=resource_data.get("mimeType")
                ))
        
        return resources
    
    def read_resource(self, uri: str) -> Dict[str, Any]:
        """
        Read a specific resource by URI.
        
        Args:
            uri: Resource URI
            
        Returns:
            Resource contents
            
        Raises:
            RuntimeError: If session not initialized
        """
        if not self.session_id:
            raise RuntimeError("Session not initialized. Call initialize() first.")
        
        body = {
            "jsonrpc": "2.0",
            "method": "resources/read",
            "params": {"uri": uri},
            "id": self._next_id,
        }
        
        response = self._post(body)
        self._next_id += 1
        return response.json()
    
    def ping(self) -> Dict[str, Any]:
        """
        Check if the session is still alive.
        
        Returns:
            Ping response
            
        Raises:
            RuntimeError: If session not initialized
        """
        if not self.session_id:
            raise RuntimeError("Session not initialized. Call initialize() first.")
        
        body = {
            "jsonrpc": "2.0",
            "method": "ping",
            "params": {},
            "id": self._next_id,
        }
        
        response = self._post(body)
        self._next_id += 1
        return response.json()
    
    def terminate(self) -> None:
        """
        Terminate the MCP session.
        
        Raises:
            requests.HTTPError: If the termination request fails
        """
        if not self.session_id:
            return  # Already terminated
        
        response = requests.delete(f"{self.base_url}/mcp", headers={
            "MCP-Session-Id": self.session_id
        })
        response.raise_for_status()
        
        # Clear session state
        self.session_id = None
        self.headers.pop("MCP-Session-Id", None)
        self.server_info = None
        self.capabilities = None
        self.protocol_version = None
    
    def __enter__(self):
        """Context manager entry."""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - automatically terminate session."""
        if self.session_id:
            try:
                self.terminate()
            except Exception:
                pass  # Ignore errors during cleanup
    
    def __repr__(self):
        """String representation of the client."""
        status = "connected" if self.session_id else "disconnected"
        return f"MoteMCPClient(base_url='{self.base_url}', status={status})"


# Convenience functions for common workflows

def register_agent_workflow(base_url: str, public_key: str) -> tuple[str, str]:
    """
    Convenience function to register an agent and return the agent_id and challenge.
    
    Args:
        base_url: Mote server URL
        public_key: Ed25519 public key in hex format
        
    Returns:
        Tuple of (agent_id, challenge)
    """
    with MoteMCPClient(base_url) as client:
        client.initialize()
        result = client.call_tool("register_agent", {"public_key": public_key})
        
        if "error" in result:
            raise RuntimeError(f"Registration failed: {result['error']}")
        
        content = result["result"]["content"][0]["text"]
        data = json.loads(content)
        return data["agent_id"], data["challenge"]


def publish_atoms_workflow(base_url: str, agent_id: str, signature: str, atoms: List[Dict[str, Any]]) -> List[str]:
    """
    Convenience function to publish atoms.
    
    Args:
        base_url: Mote server URL
        agent_id: Registered agent ID
        signature: Request signature
        atoms: List of atom data
        
    Returns:
        List of published atom IDs
    """
    with MoteMCPClient(base_url) as client:
        client.initialize()
        result = client.call_tool("publish_atoms", {
            "agent_id": agent_id,
            "signature": signature,
            "atoms": atoms
        })
        
        if "error" in result:
            raise RuntimeError(f"Publishing failed: {result['error']}")
        
        content = result["result"]["content"][0]["text"]
        data = json.loads(content)
        return data.get("published_atoms", [])


if __name__ == "__main__":
    # Example usage
    print("🚀 Mote MCP Client Example")
    print("=" * 40)
    
    try:
        with MoteMCPClient() as client:
            # Initialize session
            print("🔌 Initializing MCP session...")
            init_result = client.initialize()
            print(f"✅ Connected to {client.server_info}")
            print(f"📋 Protocol: {client.protocol_version}")
            
            # List available tools
            print("\n🔧 Available tools:")
            tools = client.list_tools()
            for tool in tools:
                print(f"  - {tool.name}: {tool.description}")
            
            # Ping test
            print("\n🏓 Ping test:")
            ping_result = client.ping()
            print(f"✅ Ping successful")
            
    except Exception as e:
        print(f"❌ Error: {e}")
