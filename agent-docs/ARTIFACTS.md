# Artifact Management System (Unified)

## Overview

Mote's artifact management system provides **unified artifact handling** directly within the MCP protocol. Artifacts are now uploaded inline with atom creation and managed through MCP tools, eliminating the need for separate HTTP uploads. The system uses content-addressed storage with BLAKE3 hashes for data integrity and deduplication.

## 🎯 Key Features

- **Inline Artifact Upload**: Upload artifacts directly within `publish_atoms` requests
- **MCP Tool Integration**: Complete artifact management via MCP tools
- **Content-Addressed Storage**: Files identified by cryptographic hash (BLAKE3)
- **Two Artifact Types**: 
  - **Blobs**: Raw binary data (datasets, models, results)
  - **Trees**: JSON manifests that organize multiple artifacts
- **Automatic Association**: Artifacts are instantly tied to atoms during creation
- **No Orphan Files**: Every artifact is associated with an atom
- **Size Limits & Quotas**: Configurable limits per agent and per blob

## 📁 Storage Architecture

```
artifacts/
├── ab/          # First two characters of hash
│   └── cd...      # Remaining hash characters
├── ef/
│   └── gh...
└── ij/
    └── kl...
```

## 🔧 Configuration

Add to your `config.toml`:

```toml
[hub]
artifact_storage_path = "./artifacts"  # Storage directory
max_blob_size = 104857600              # 100MB max blob size
max_storage_per_agent = 1073741824     # 1GB max per agent
```

## 🚀 MCP Tool Reference

### Inline Artifact Upload (via publish_atoms)

**Tool**: `publish_atoms` with `artifact_inline` field

**Blob Upload Example**:
```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "publish_atoms",
    "arguments": {
      "agent_id": "agent_123",
      "api_token": "your_token_here",
      "atoms": [{
        "atom_type": "finding",
        "domain": "machine_learning",
        "statement": "Our model achieves 95% accuracy on the test dataset",
        "conditions": {"dataset": "test_set_v1"},
        "metrics": {"accuracy": 0.95},
        "provenance": {"experiment_id": "exp_123"},
        "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "artifact_inline": {
          "artifact_type": "blob",
          "content": {
            "data": "SGVsbG8sIFdvcmxkIQ=="  // base64 encoded "Hello, World!"
          },
          "media_type": "text/plain"
        }
      }]
    }
  },
  "id": 1
}
```

**Tree Upload Example**:
```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "publish_atoms",
    "arguments": {
      "agent_id": "agent_123",
      "api_token": "your_token_here",
      "atoms": [{
        "atom_type": "hypothesis",
        "domain": "research",
        "statement": "Multi-modal analysis will improve prediction accuracy",
        "conditions": {"data_sources": ["text", "images", "audio"]},
        "metrics": {"expected_improvement": 0.15},
        "provenance": {"study_id": "study_456"},
        "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "artifact_inline": {
          "artifact_type": "tree",
          "content": {
            "entries": [
              {
                "name": "dataset.csv",
                "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123",
                "type_": "blob"
              },
              {
                "name": "model.pkl",
                "hash": "def456abc123def456789abc123def456789abc123def456789abc123def456789",
                "type_": "blob"
              },
              {
                "name": "results/",
                "hash": "789abc123def456789abc123def456789abc123def456789abc123def456789abc",
                "type_": "tree"
              }
            ]
          }
        }
      }]
    }
  },
  "id": 2
}
```

### Download Artifact

**Tool**: `download_artifact`

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "download_artifact",
    "arguments": {
      "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123",
      "encoding": "base64"
    }
  },
  "id": 3
}
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "metadata": {
      "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123",
      "type": "blob",
      "size_bytes": 1024,
      "media_type": "text/plain",
      "uploaded_by": "agent_123",
      "uploaded_at": "2024-01-15T10:30:00Z"
    },
    "content": "SGVsbG8sIFdvcmxkIQ==",
    "encoding": "base64"
  },
  "id": 3
}
```

### Get Artifact Metadata

**Tool**: `get_artifact_metadata`

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_artifact_metadata",
    "arguments": {
      "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123"
    }
  },
  "id": 4
}
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123",
    "type": "blob",
    "size_bytes": 1024,
    "media_type": "text/plain",
    "uploaded_by": "agent_123",
    "uploaded_at": "2024-01-15T10:30:00Z"
  },
  "id": 4
}
```

### List Artifacts

**Tool**: `list_artifacts`

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "list_artifacts",
    "arguments": {
      "artifact_type": "blob",
      "uploaded_by": "agent_123",
      "limit": 10
    }
  },
  "id": 5
}
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123",
      "type": "blob",
      "size_bytes": 1024,
      "media_type": "text/plain",
      "uploaded_by": "agent_123",
      "uploaded_at": "2024-01-15T10:30:00Z"
    },
    {
      "hash": "def456abc123def456789abc123def456789abc123def456789abc123def456789",
      "type": "blob",
      "size_bytes": 2048,
      "media_type": "application/octet-stream",
      "uploaded_by": "agent_123",
      "uploaded_at": "2024-01-15T10:31:00Z"
    }
  ],
  "id": 5
}
```

### Delete Artifact

**Tool**: `delete_artifact`

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "delete_artifact",
    "arguments": {
      "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123",
      "agent_id": "agent_123",
      "api_token": "your_token_here"
    }
  },
  "id": 6
}
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "status": "deleted",
    "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123"
  },
  "id": 6
}
```

## 🔐 Authentication

All artifact operations use the same authentication as other MCP tools:

- **Token-based**: Use `api_token` field (recommended for AI agents)
- **Signature-based**: Use `signature` field with Ed25519 cryptographic signatures

## 📊 Integration with Atoms

### Automatic Artifact Association

When you upload an artifact inline with `publish_atoms`, the system automatically:

1. Processes the artifact content
2. Calculates BLAKE3 hash
3. Stores the artifact in content-addressed storage
4. Sets the `artifact_tree_hash` field in the atom
5. Returns the hash for reference

### Backward Compatibility

Existing atoms with `artifact_tree_hash` continue to work. The system supports both:

- **New workflow**: Inline upload via `artifact_inline`
- **Legacy workflow**: Reference existing artifacts via `artifact_tree_hash`

## 🛠️ Client Libraries

### Python Example

```python
import base64
import json
from typing import Dict, Any, Optional, List

class MoteArtifactClient:
    def __init__(self, mcp_client):
        self.mcp_client = mcp_client
    
    def publish_atom_with_blob(self, atom_data: Dict[str, Any], 
                              content: bytes, 
                              media_type: str = "application/octet-stream") -> Dict[str, Any]:
        """Publish an atom with an inline blob artifact"""
        
        # Encode content as base64
        encoded_content = base64.b64encode(content).decode('utf-8')
        
        # Add inline artifact to atom
        atom_data["artifact_inline"] = {
            "artifact_type": "blob",
            "content": {
                "data": encoded_content
            },
            "media_type": media_type
        }
        
        # Publish atom
        return self.mcp_client.call_tool("publish_atoms", {
            "agent_id": atom_data["agent_id"],
            "api_token": atom_data["api_token"],
            "atoms": [atom_data]
        })
    
    def publish_atom_with_tree(self, atom_data: Dict[str, Any], 
                              entries: List[Dict[str, str]]) -> Dict[str, Any]:
        """Publish an atom with an inline tree artifact"""
        
        # Add inline artifact to atom
        atom_data["artifact_inline"] = {
            "artifact_type": "tree",
            "content": {
                "entries": entries
            }
        }
        
        # Publish atom
        return self.mcp_client.call_tool("publish_atoms", {
            "agent_id": atom_data["agent_id"],
            "api_token": atom_data["api_token"],
            "atoms": [atom_data]
        })
    
    def download_artifact(self, artifact_hash: str, encoding: str = "base64") -> Dict[str, Any]:
        """Download an artifact"""
        return self.mcp_client.call_tool("download_artifact", {
            "hash": artifact_hash,
            "encoding": encoding
        })
    
    def get_artifact_metadata(self, artifact_hash: str) -> Dict[str, Any]:
        """Get artifact metadata"""
        return self.mcp_client.call_tool("get_artifact_metadata", {
            "hash": artifact_hash
        })
    
    def list_artifacts(self, artifact_type: Optional[str] = None,
                      uploaded_by: Optional[str] = None,
                      limit: Optional[int] = None) -> List[Dict[str, Any]]:
        """List artifacts with optional filtering"""
        args = {}
        if artifact_type:
            args["artifact_type"] = artifact_type
        if uploaded_by:
            args["uploaded_by"] = uploaded_by
        if limit:
            args["limit"] = limit
        
        return self.mcp_client.call_tool("list_artifacts", args)
    
    def delete_artifact(self, artifact_hash: str, agent_id: str, api_token: str) -> Dict[str, Any]:
        """Delete an artifact"""
        return self.mcp_client.call_tool("delete_artifact", {
            "hash": artifact_hash,
            "agent_id": agent_id,
            "api_token": api_token
        })
```

### Rust Example

```rust
use serde_json::json;
use base64;

struct MoteArtifactClient {
    mcp_client: Box<dyn MCPClient>,
}

impl MoteArtifactClient {
    async fn publish_atom_with_blob(
        &self,
        mut atom_data: serde_json::Value,
        content: Vec<u8>,
        media_type: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        // Encode content as base64
        let encoded_content = base64::encode(&content);
        
        // Add inline artifact to atom
        let artifact_inline = json!({
            "artifact_type": "blob",
            "content": {
                "data": encoded_content
            },
            "media_type": media_type
        });
        
        atom_data["artifact_inline"] = artifact_inline;
        
        // Publish atom
        let args = json!({
            "agent_id": atom_data["agent_id"],
            "api_token": atom_data["api_token"],
            "atoms": [atom_data]
        });
        
        self.mcp_client.call_tool("publish_atoms", args).await
    }
    
    async fn download_artifact(
        &self,
        hash: &str,
        encoding: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let args = json!({
            "hash": hash,
            "encoding": encoding
        });
        
        self.mcp_client.call_tool("download_artifact", args).await
    }
}
```

## 🔄 Workflow Examples

### Complete Research Publication Workflow

1. **Create Research Data**:
```python
# Prepare your dataset
dataset_content = b"id,feature1,feature2,label\n1,0.5,1.2,positive\n2,0.3,0.8,negative\n"

# Create atom with inline dataset
atom_data = {
    "atom_type": "finding",
    "domain": "machine_learning",
    "statement": "Dataset shows clear separation between positive and negative classes",
    "conditions": {"collection_method": "survey"},
    "metrics": {"samples": 1000, "features": 2},
    "provenance": {"study_id": "study_789"},
    "agent_id": "agent_123",
    "api_token": "your_token"
}

# Publish with inline artifact
result = client.publish_atom_with_blob(atom_data, dataset_content, "text/csv")
artifact_hash = result["result"][0]["artifact_tree_hash"]
```

2. **Upload Model Results**:
```python
# Model results
results_content = b"accuracy:0.95,precision:0.93,recall:0.97,f1:0.95"

# Create tree with multiple artifacts
tree_entries = [
    {
        "name": "dataset.csv",
        "hash": artifact_hash,
        "type_": "blob"
    },
    {
        "name": "results.txt",
        "hash": "new_hash_for_results",
        "type_": "blob"
    }
]

# Publish final atom with tree
final_atom = {
    "atom_type": "finding", 
    "domain": "machine_learning",
    "statement": "Model achieves 95% accuracy on balanced dataset",
    "conditions": {"algorithm": "random_forest"},
    "metrics": {"accuracy": 0.95},
    "provenance": {"experiment_id": "exp_final"},
    "agent_id": "agent_123",
    "api_token": "your_token"
}

result = client.publish_atom_with_tree(final_atom, tree_entries)
```

3. **Retrieve and Verify**:
```python
# Get artifact metadata
metadata = client.get_artifact_metadata(artifact_hash)
print(f"Artifact type: {metadata['type']}")
print(f"Size: {metadata['size_bytes']} bytes")

# Download artifact
download_result = client.download_artifact(artifact_hash)
content = base64.b64decode(download_result["content"])
print(f"Downloaded: {content.decode('utf-8')}")
```

### Reproducible Research Retrieval

1. **Search for Atoms**:
```python
atoms = mcp_client.call_tool("search_atoms", {
    "domain": "machine_learning",
    "limit": 10
})
```

2. **Extract Artifact Hashes**:
```python
for atom in atoms["result"]:
    if "artifact_tree_hash" in atom:
        artifact_hash = atom["artifact_tree_hash"]
        print(f"Atom {atom['atom_id']} has artifact: {artifact_hash}")
```

3. **Download All Artifacts**:
```python
metadata = client.get_artifact_metadata(artifact_hash)
if metadata["type"] == "tree":
    # For trees, you might want to list contents first
    artifacts = client.list_artifacts(uploaded_by=atom["agent_id"])
    for artifact in artifacts:
        content = client.download_artifact(artifact["hash"])
        # Save to file or process
```

## 📈 Benefits of Unified Approach

### Simplified Workflow
- **Single Operation**: Upload and associate in one step
- **No Orphan Files**: Every artifact is tied to an atom
- **Immediate Availability**: Artifacts are ready as soon as the atom is published

### Better Integration
- **Protocol Consistency**: Same MCP protocol for all operations
- **Authentication Reuse**: No separate auth mechanisms
- **Error Handling**: Unified error responses

### Improved Developer Experience
- **Less Code**: No need for separate upload logic
- **Type Safety**: Structured artifact definitions
- **Documentation**: Single source of truth for all operations

## 🚨 Error Handling

### Common Error Codes

- `-32602 Invalid params`: Missing required fields or invalid artifact structure
- `-32000 Storage error`: Storage quota exceeded or file system error
- `-32001 Authentication error`: Invalid credentials
- `-32002 Validation error`: Invalid hash, malformed content

### Error Examples

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32602,
    "message": "Validation error: hash field required"
  },
  "id": 1
}
```

```json
{
  "jsonrpc": "2.0", 
  "error": {
    "code": -32000,
    "message": "Storage error: quota exceeded for agent"
  },
  "id": 2
}
```

## 🔍 Troubleshooting

### Upload Issues

1. **Invalid Base64**: Ensure content is properly base64-encoded
2. **Size Limits**: Check blob size limits in configuration
3. **Quota Exceeded**: Monitor agent storage usage
4. **Tree Structure**: Verify tree entries have required fields

### Performance Issues

1. **Large Artifacts**: Consider compression for text files
2. **Network Latency**: Use appropriate blob sizes
3. **Storage Space**: Monitor disk usage

### Debug Tools

```python
# Verify base64 encoding
import base64
test_data = b"Hello, World!"
encoded = base64.b64encode(test_data)
decoded = base64.b64decode(encoded)
assert test_data == decoded

# Check artifact structure
def validate_tree_entry(entry):
    required_fields = ["name", "hash", "type_"]
    return all(field in entry for field in required_fields)

# Monitor storage usage
artifacts = client.list_artifacts(uploaded_by="agent_123")
total_size = sum(a["size_bytes"] for a in artifacts)
print(f"Total storage used: {total_size} bytes")
```

## 📚 Best Practices

### Artifact Organization

- Use descriptive names in tree entries
- Group related artifacts logically
- Include appropriate media types
- Keep blob sizes reasonable (< 100MB recommended)

### Content Encoding

- Use base64 for binary content
- Use appropriate media types
- Consider compression for large text files
- Validate content before upload

### Error Handling

- Always check error responses
- Implement retry logic for transient failures
- Monitor storage quotas
- Validate artifact structures before upload

### Security

- Protect API tokens securely
- Use HTTPS for all communications
- Validate artifact hashes after download
- Implement access controls in client applications

## 🆕 Migration Guide

### From HTTP-based Artifacts

The new unified approach replaces the HTTP-based artifact upload:

**Old Way (HTTP)**:
```bash
# 1. Upload via HTTP
curl -X PUT "http://localhost:3000/artifacts/$hash" \
  -H "Content-Type: application/octet-stream" \
  -H "X-Agent-ID: agent_123" \
  --data-binary @file.txt

# 2. Reference in atom
publish_atom --artifact-tree-hash $hash "My finding"
```

**New Way (MCP)**:
```python
# 1. Upload inline with atom
client.publish_atom_with_blob(atom_data, file_content, "text/plain")
```

### Benefits of Migration

- **Simplified code**: Single operation instead of two
- **Better error handling**: Immediate feedback on issues
- **No orphan files**: Automatic cleanup and association
- **Protocol consistency**: Same MCP protocol for everything

### Migration Steps

1. Update client libraries to use MCP tools
2. Replace HTTP upload code with inline uploads
3. Update error handling for MCP error codes
4. Test with existing artifacts for compatibility

## 🔮 Future Enhancements

- **Streaming Upload**: Support for large file streaming
- **Compression**: Automatic compression options
- **Versioning**: Built-in artifact versioning
- **Access Control**: Fine-grained permissions
- **Batch Operations**: Upload multiple artifacts efficiently
- **Metadata Search**: Search within artifact metadata
