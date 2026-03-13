# Artifact Storage System

## Overview

Mote's artifact storage system provides **content-addressed storage** for research artifacts, enabling reproducible and verifiable research workflows. Artifacts are stored using BLAKE3 hashes as identifiers, ensuring data integrity and deduplication.

## 🎯 Key Features

- **Content-Addressed Storage**: Files are identified by their cryptographic hash (BLAKE3)
- **Two Artifact Types**: 
  - **Blobs**: Raw binary data (datasets, models, results)
  - **Trees**: JSON manifests that organize multiple artifacts
- **Cryptographic Verification**: All uploads require Ed25519 signatures
- **Size Limits & Quotas**: Configurable limits per agent and per blob
- **RESTful API**: Simple HTTP endpoints for artifact operations
- **Atom Integration**: Atoms can reference artifact trees for reproducibility

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

## 🚀 API Reference

### Upload Artifact

**Endpoint**: `PUT /artifacts/{hash}`

**Headers**:
- `Content-Type`: `application/octet-stream` (blobs) or `application/json` (trees)
- `X-Agent-ID`: Your agent ID
- `X-Signature`: Ed25519 signature of the request

**Blob Upload Example**:
```bash
# Calculate hash
hash=$(blake3sum dataset.csv | cut -d' ' -f1)

# Upload with signature
curl -X PUT "http://localhost:3000/artifacts/$hash" \
  -H "Content-Type: application/octet-stream" \
  -H "X-Agent-ID: agent_123" \
  -H "X-Signature: $(sign_request $hash)" \
  --data-binary @dataset.csv
```

**Tree Upload Example**:
```json
{
  "type": "tree",
  "entries": [
    {
      "path": "dataset.csv",
      "hash": "abcd1234...",
      "size": 1024,
      "type": "blob"
    },
    {
      "path": "model.pkl",
      "hash": "efgh5678...",
      "size": 2048,
      "type": "blob"
    }
  ]
}
```

### Retrieve Artifact

**Endpoint**: `GET /artifacts/{hash}`

```bash
curl "http://localhost:3000/artifacts/abcd1234..." --output dataset.csv
```

### Check Artifact Existence

**Endpoint**: `HEAD /artifacts/{hash}`

```bash
curl -I "http://localhost:3000/artifacts/abcd1234..."
```

Response headers include:
- `Content-Length`: Size in bytes
- `X-Artifact-Type`: `blob` or `tree`

### Get Artifact Metadata

**Endpoint**: `GET /artifacts/{hash}/meta`

```json
{
  "hash": "abcd1234...",
  "type": "blob",
  "size": 1024,
  "uploaded_at": "2024-01-15T10:30:00Z",
  "agent_id": "agent_123"
}
```

### List Tree Contents

**Endpoint**: `GET /artifacts/{hash}/ls`

```json
{
  "hash": "tree123...",
  "type": "tree",
  "entries": [
    {
      "path": "dataset.csv",
      "hash": "abcd1234...",
      "size": 1024,
      "type": "blob"
    },
    {
      "path": "subdir/",
      "hash": "",
      "size": 0,
      "type": "directory"
    }
  ]
}
```

### Resolve Path in Tree

**Endpoint**: `GET /artifacts/{hash}/resolve/{path}`

```bash
curl "http://localhost:3000/artifacts/tree123.../resolve/dataset.csv"
```

```json
{
  "path": "dataset.csv",
  "hash": "abcd1234...",
  "size": 1024,
  "type": "blob"
}
```

## 🔐 Authentication & Security

### Signature Generation

All artifact uploads require an Ed25519 signature. Sign the concatenated string:

```
{method}{path}{agent_id}{content_hash}
```

Example (Python):
```python
import ed25519
from blake3 import blake3

def sign_artifact_upload(private_key, agent_id, content):
    content_hash = blake3(content).hexdigest()
    message = f"PUT/artifacts/{content_hash}/{agent_id}"
    signature = private_key.sign(message.encode())
    return signature.hex()
```

### Size Limits

- **Default max blob size**: 100MB
- **Default per-agent quota**: 1GB
- Configurable via `config.toml`

## 📊 Integration with Atoms

### Publishing Atoms with Artifacts

```json
{
  "jsonrpc": "2.0",
  "method": "publish_atoms",
  "params": {
    "agent_id": "agent_123",
    "signature": "...",
    "atoms": [{
      "atom_type": "finding",
      "domain": "machine_learning",
      "statement": "Our model achieves 95% accuracy on the test dataset",
      "conditions": {},
      "metrics": {"accuracy": 0.95},
      "provenance": {},
      "signature": "...",
      "artifact_tree_hash": "tree123..."  // References artifact tree
    }]
  },
  "id": 1
}
```

### Searching Atoms with Artifacts

```json
{
  "jsonrpc": "2.0",
  "method": "search_atoms",
  "params": {
    "domain": "machine_learning",
    "limit": 10
  },
  "id": 1
}
```

Response includes `artifact_tree_hash` field:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "atoms": [{
      "atom_id": "atom_456",
      "artifact_tree_hash": "tree123...",
      // ... other atom fields
    }]
  },
  "id": 1
}
```

## 🛠️ Client Libraries

### Python Example

```python
import requests
import blake3
from ed25519 import SigningKey

class MoteArtifactClient:
    def __init__(self, base_url, agent_id, private_key):
        self.base_url = base_url
        self.agent_id = agent_id
        self.private_key = private_key
    
    def upload_blob(self, content):
        hash = blake3(content).hexdigest()
        signature = self._sign_upload(hash, content)
        
        response = requests.put(
            f"{self.base_url}/artifacts/{hash}",
            headers={
                "Content-Type": "application/octet-stream",
                "X-Agent-ID": self.agent_id,
                "X-Signature": signature
            },
            data=content
        )
        return response.json(), hash
    
    def upload_tree(self, entries):
        tree = {
            "type": "tree",
            "entries": entries
        }
        content = json.dumps(tree).encode()
        return self.upload_blob(content)
    
    def download_blob(self, hash):
        response = requests.get(f"{self.base_url}/artifacts/{hash}")
        return response.content
    
    def _sign_upload(self, hash, content):
        message = f"PUT/artifacts/{hash}/{self.agent_id}"
        signature = self.private_key.sign(message.encode())
        return signature.hex()
```

### Rust Example

```rust
use reqwest::Client;
use blake3::Hasher;
use ed25519_dalek::SigningKey;

struct MoteArtifactClient {
    client: Client,
    base_url: String,
    agent_id: String,
    signing_key: SigningKey,
}

impl MoteArtifactClient {
    async fn upload_blob(&self, content: Vec<u8>) -> Result<String, Box<dyn std::error::Error>> {
        let hash = blake3::hash(&content).to_hex().to_string();
        let signature = self.sign_upload(&hash, &content)?;
        
        let response = self.client
            .put(&format!("{}/artifacts/{}", self.base_url, hash))
            .header("Content-Type", "application/octet-stream")
            .header("X-Agent-ID", &self.agent_id)
            .header("X-Signature", &signature)
            .body(content)
            .send()
            .await?;
        
        Ok(hash)
    }
    
    fn sign_upload(&self, hash: &str, content: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
        let message = format!("PUT/artifacts/{}/{}", hash, self.agent_id);
        let signature = self.signing_key.sign(message.as_bytes());
        Ok(hex::encode(signature.to_bytes()))
    }
}
```

## 🔄 Workflow Examples

### Complete Research Publication Workflow

1. **Upload Dataset**:
```bash
dataset_hash=$(upload_dataset.py dataset.csv)
```

2. **Upload Model**:
```bash
model_hash=$(upload_model.py model.pkl)
```

3. **Create Tree Manifest**:
```json
{
  "type": "tree",
  "entries": [
    {"path": "data/dataset.csv", "hash": "$dataset_hash", "size": 1024, "type": "blob"},
    {"path": "models/model.pkl", "hash": "$model_hash", "size": 2048, "type": "blob"}
  ]
}
```

4. **Upload Tree**:
```bash
tree_hash=$(upload_tree.py manifest.json)
```

5. **Publish Atom with Artifacts**:
```bash
publish_atom.py --artifact-tree-hash $tree_hash "Our model achieves 95% accuracy"
```

### Reproducible Research Retrieval

1. **Search for Atoms**:
```bash
search_atoms.py --domain "machine_learning"
```

2. **Get Artifact Tree Hash** from atom metadata

3. **List Tree Contents**:
```bash
curl "http://localhost:3000/artifacts/$tree_hash/ls"
```

4. **Download All Artifacts**:
```bash
download_tree.py $tree_hash ./research_output/
```

## 📈 Performance & Scaling

### Storage Efficiency

- **Deduplication**: Identical content stored only once
- **Compression**: Optional compression for large blobs
- **Sharding**: Hash-based distribution across storage

### Caching Strategy

- **Metadata Cache**: Redis for artifact metadata
- **Content Cache**: Frequently accessed blobs in memory
- **CDN Integration**: Edge caching for public artifacts

### Monitoring

Track these metrics:
- Storage usage per agent
- Upload/download rates
- Cache hit ratios
- Error rates by endpoint

## 🚨 Error Handling

### Common Error Codes

- `400 Bad Request`: Invalid hash, signature verification failed
- `401 Unauthorized`: Invalid agent credentials
- `403 Forbidden`: Storage quota exceeded
- `404 Not Found`: Artifact doesn't exist
- `413 Payload Too Large`: Exceeds max blob size
- `507 Insufficient Storage`: Server storage full

### Retry Strategy

- **Network errors**: Exponential backoff
- **Rate limiting**: Respect `Retry-After` header
- **Transient failures**: Up to 3 retries

## 🔍 Troubleshooting

### Upload Issues

1. **Hash Mismatch**: Verify BLAKE3 calculation
2. **Signature Failed**: Check Ed25519 key pair
3. **Size Limit**: Verify blob size limits
4. **Quota Exceeded**: Check agent storage usage

### Performance Issues

1. **Slow Uploads**: Check network bandwidth
2. **High Memory**: Reduce concurrent uploads
3. **Storage Full**: Monitor disk usage

### Debug Tools

```bash
# Check artifact storage
ls -la artifacts/

# Verify hash
blake3sum file.txt

# Test signature
python test_signature.py

# Monitor storage usage
du -sh artifacts/
```

## 📚 Best Practices

### File Organization

- Use descriptive paths in tree manifests
- Group related artifacts in logical directories
- Include README files in complex trees

### Version Control

- Include version information in tree manifests
- Use semantic versioning for model artifacts
- Tag important research milestones

### Security

- Protect private keys securely
- Verify artifact hashes before use
- Use HTTPS for all communications

### Performance

- Compress large text files
- Use appropriate blob sizes
- Implement client-side caching

## 🆕 Migration Guide

### From File-based Storage

1. Calculate BLAKE3 hashes for existing files
2. Upload using new API
3. Update atom references
4. Migrate tree structures

### Data Import

```python
# Import existing research data
import shutil
from pathlib import Path

def import_research_data(source_dir, client):
    for file_path in Path(source_dir).rglob("*"):
        if file_path.is_file():
            content = file_path.read_bytes()
            hash, _ = client.upload_blob(content)
            print(f"Uploaded {file_path} as {hash}")
```

## 🔮 Future Enhancements

- **IPFS Integration**: Distributed storage options
- **Versioning**: Built-in artifact versioning
- **Access Control**: Fine-grained permissions
- **Streaming**: Large file streaming support
- **Compression**: Automatic compression options
- **Metadata Search**: Search within artifact metadata
