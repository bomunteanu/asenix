#!/usr/bin/env python3
"""
Simple test agent for Mote infrastructure
"""
import requests
import json
import time
import hashlib
from cryptography.hazmat.primitives.asymmetric import ed25519
import binascii

class MoteAgent:
    def __init__(self, base_url="http://localhost:3000"):
        self.base_url = base_url
        self.mcp_url = f"{base_url}/mcp"
        self.agent_id = None
        self.private_key = ed25519.Ed25519PrivateKey.generate()
        self.public_key = self.private_key.public_key()
        
    def make_mcp_request(self, method, params=None, request_id=1):
        """Make a JSON-RPC 2.0 request to Mote"""
        payload = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or {},
            "id": request_id
        }
        
        response = requests.post(self.mcp_url, json=payload)
        response.raise_for_status()
        return response.json()
    
    def register(self):
        """Register the agent with Mote"""
        print("🔐 Registering agent...")
        public_key_hex = binascii.hexlify(self.public_key.public_bytes_raw()).decode()
        
        response = self.make_mcp_request("register_agent", {
            "public_key": public_key_hex
        })
        
        if "result" in response:
            self.agent_id = response["result"]["agent_id"]
            self.challenge = response["result"]["challenge"]
            print(f"✅ Registered as agent: {self.agent_id}")
            return True
        else:
            print(f"❌ Registration failed: {response}")
            return False
    
    def confirm(self):
        """Confirm agent registration by signing challenge"""
        print("🔑 Confirming registration...")
        challenge_bytes = binascii.unhexlify(self.challenge)
        signature = self.private_key.sign(challenge_bytes)
        signature_hex = binascii.hexlify(signature).decode()
        
        response = self.make_mcp_request("confirm_agent", {
            "agent_id": self.agent_id,
            "signature": signature_hex
        })
        
        if "result" in response and response["result"]["status"] == "confirmed":
            print("✅ Agent confirmed")
            return True
        else:
            print(f"❌ Confirmation failed: {response}")
            return False
    
    def publish_bounty(self):
        """Publish a bounty atom"""
        print("🎯 Publishing bounty...")
        
        # Create atom signature (mock for testing)
        atom_signature = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
        atom_signature_bytes = binascii.unhexlify(atom_signature)
        
        # Prepare the atom data
        atom_data = {
            "atom_type": "bounty",
            "domain": "machine_learning",
            "statement": "Improve accuracy of image classification models on medical datasets",
            "conditions": {
                "task": "image_classification",
                "dataset_type": "medical",
                "target_metric": "accuracy",
                "min_improvement": 0.05
            },
            "metrics": None,
            "provenance": {
                "parent_ids": [],
                "code_hash": None,
                "environment": None,
                "dataset_fingerprint": None,
                "experiment_ref": None,
                "method_description": None
            },
            "signature": list(atom_signature_bytes)
        }
        
        # Prepare request with atom signature
        request_data = {
            "agent_id": self.agent_id,
            "atoms": [atom_data]
        }
        
        # Generate top-level signature
        canonical_json = json.dumps(request_data, separators=(',', ':'), sort_keys=True)
        top_signature = self.private_key.sign(canonical_json.encode())
        top_signature_hex = binascii.hexlify(top_signature).decode()
        
        # Final request
        final_request = {
            "agent_id": self.agent_id,
            "signature": top_signature_hex,
            "atoms": [atom_data]
        }
        
        response = self.make_mcp_request("publish_atoms", final_request)
        
        if "result" in response:
            atom_ids = response["result"].get("atom_ids", [])
            print(f"✅ Published bounty with atom IDs: {atom_ids}")
            return atom_ids
        else:
            print(f"❌ Bounty publication failed: {response}")
            return []
    
    def get_suggestions(self):
        """Get suggestions from Mote"""
        print("🔍 Getting suggestions...")
        
        response = self.make_mcp_request("get_suggestions", {
            "context": {"domain": "machine_learning"},
            "k": 5
        })
        
        if "result" in response:
            suggestions = response["result"].get("suggestions", [])
            print(f"✅ Got {len(suggestions)} suggestions:")
            for i, suggestion in enumerate(suggestions):
                print(f"  {i+1}. {suggestion.get('statement', 'N/A')}")
            return suggestions
        else:
            print(f"❌ Failed to get suggestions: {response}")
            return []
    
    def search_atoms(self):
        """Search for atoms"""
        print("🔎 Searching atoms...")
        
        response = self.make_mcp_request("search_atoms", {
            "domain": "machine_learning",
            "atom_types": ["bounty", "finding"]
        })
        
        if "result" in response:
            atoms = response["result"].get("atoms", [])
            print(f"✅ Found {len(atoms)} atoms:")
            for atom in atoms:
                print(f"  - {atom.get('type', 'N/A')}: {atom.get('statement', 'N/A')}")
            return atoms
        else:
            print(f"❌ Search failed: {response}")
            return []

def main():
    """Run the test agent"""
    print("🚀 Starting Mote Test Agent")
    print("=" * 50)
    
    agent = MoteAgent()
    
    # Step 1: Register and confirm
    if not agent.register():
        return
    if not agent.confirm():
        return
    
    print("\n" + "=" * 50)
    
    # Step 2: Publish a bounty
    bounty_ids = agent.publish_bounty()
    
    print("\n" + "=" * 50)
    
    # Step 3: Wait a moment for processing
    print("⏳ Waiting for processing...")
    time.sleep(2)
    
    # Step 4: Get suggestions
    agent.get_suggestions()
    
    print("\n" + "=" * 50)
    
    # Step 5: Search atoms
    agent.search_atoms()
    
    print("\n" + "=" * 50)
    print("✅ Test completed successfully!")

if __name__ == "__main__":
    main()