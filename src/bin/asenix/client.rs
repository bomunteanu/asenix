use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

// ── Project types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct ProjectInfo {
    pub project_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProjectFileInfo {
    pub filename: String,
    pub size_bytes: i64,
    pub content_type: Option<String>,
    pub uploaded_at: String,
}

pub struct AsenixClient {
    pub hub: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub database: String,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub embedding_queue_depth: usize,
}

#[derive(Debug, Deserialize)]
pub struct ReviewItem {
    pub atom_id: String,
    pub atom_type: String,
    pub domain: String,
    pub statement: String,
    pub author_agent_id: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ReviewQueue {
    pub items: Vec<ReviewItem>,
    pub total: u64,
}

#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    pub agent_id: String,
    pub api_token: String,
}

impl AsenixClient {
    pub fn new(hub: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            hub: hub.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/health", self.hub);
        let resp = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("cannot reach {}", self.hub))?;
        if !resp.status().is_success() {
            return Err(anyhow!("health check returned HTTP {}", resp.status()));
        }
        resp.json::<HealthResponse>()
            .context("unexpected health response format")
    }

    pub fn admin_login(&self, secret: &str) -> Result<String> {
        #[derive(Serialize)]
        struct Req<'a> {
            secret: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            token: String,
        }
        let url = format!("{}/admin/login", self.hub);
        let resp = self
            .client
            .post(&url)
            .json(&Req { secret })
            .send()
            .context("cannot reach hub")?;
        if resp.status().as_u16() == 401 {
            return Err(anyhow!("invalid secret"));
        }
        if !resp.status().is_success() {
            return Err(anyhow!("login failed: HTTP {}", resp.status()));
        }
        let body: Resp = resp.json().context("unexpected login response format")?;
        Ok(body.token)
    }

    pub fn get_review_queue(&self, token: &str) -> Result<ReviewQueue> {
        let url = format!("{}/review?limit=50", self.hub);
        let resp = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .context("cannot reach hub")?;
        if resp.status().as_u16() == 401 {
            return Err(anyhow!("unauthorized — token may have expired"));
        }
        if !resp.status().is_success() {
            return Err(anyhow!("review queue request failed: HTTP {}", resp.status()));
        }
        resp.json::<ReviewQueue>()
            .context("unexpected review queue response format")
    }

    pub fn post_review(
        &self,
        token: &str,
        id: &str,
        action: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Req<'a> {
            action: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            reason: Option<&'a str>,
        }
        let url = format!("{}/review/{}", self.hub, id);
        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .json(&Req { action, reason })
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("review action failed: {}", body));
        }
        Ok(())
    }

    /// Register an agent via the simple REST endpoint (no Ed25519).
    pub fn register_simple(&self, name: &str) -> Result<RegisterResponse> {
        #[derive(Serialize)]
        struct Req<'a> {
            agent_name: &'a str,
        }
        let url = format!("{}/register", self.hub);
        let resp = self
            .client
            .post(&url)
            .json(&Req { agent_name: name })
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("registration failed (HTTP {}): {}", status, body));
        }
        resp.json::<RegisterResponse>()
            .context("unexpected registration response format")
    }

    /// Call an MCP tool. Opens a fresh session, invokes the tool, returns the parsed result.
    pub fn mcp_call(&self, tool: &str, args: Value) -> Result<Value> {
        let url = format!("{}/mcp", self.hub);

        // 1. Initialize session
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "asenix-cli", "version": "0.1.0" }
            }
        });
        let init_resp = self
            .client
            .post(&url)
            .json(&init_req)
            .send()
            .context("cannot reach MCP endpoint")?;

        // Extract session ID before consuming response body
        let session_id = init_resp
            .headers()
            .get("mcp-session-id")
            .context("hub did not return an MCP session ID — is the server running?")?
            .to_str()
            .context("invalid MCP session ID header")?
            .to_string();
        // Drain the init body
        let _: Value = init_resp.json().unwrap_or(Value::Null);

        // 2. Send notifications/initialized (fire-and-forget)
        let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        let _ = self
            .client
            .post(&url)
            .header("mcp-session-id", &session_id)
            .json(&notif)
            .send();

        // 3. Call the tool
        let call_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": tool, "arguments": args }
        });
        let call_resp = self
            .client
            .post(&url)
            .header("mcp-session-id", &session_id)
            .json(&call_req)
            .send()
            .context("MCP tool call failed")?;

        if !call_resp.status().is_success() {
            let body = call_resp.text().unwrap_or_default();
            return Err(anyhow!("MCP call failed ({}): {}", tool, body));
        }

        let body: Value = call_resp
            .json()
            .context("failed to parse MCP response")?;

        if let Some(err) = body.get("error") {
            return Err(anyhow!("MCP tool '{}' returned error: {}", tool, err));
        }

        // MCP tool results are in result.content[0].text as a JSON string
        let text = body
            .pointer("/result/content/0/text")
            .and_then(|v| v.as_str())
            .context("unexpected MCP response structure (no result.content[0].text)")?;

        serde_json::from_str(text)
            .with_context(|| format!("failed to parse '{}' tool result as JSON", tool))
    }

    /// Call a JSON-RPC method on the /rpc endpoint (for internal tools like search_atoms).
    pub fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
        let url = format!("{}/rpc", self.hub);
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        let resp = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .with_context(|| format!("cannot reach /rpc for method '{}'", method))?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("rpc '{}' failed: {}", method, body));
        }
        let body: Value = resp.json().context("failed to parse RPC response")?;
        if let Some(err) = body.get("error") {
            return Err(anyhow!("rpc '{}' error: {}", method, err));
        }
        body.get("result")
            .cloned()
            .with_context(|| format!("rpc '{}' response missing 'result'", method))
    }

    // ── Project REST endpoints ─────────────────────────────────────────────────

    pub fn list_projects(&self) -> Result<Vec<ProjectInfo>> {
        let url = format!("{}/projects", self.hub);
        let resp = self.client.get(&url).send().context("cannot reach hub")?;
        if !resp.status().is_success() {
            return Err(anyhow!("list_projects failed: HTTP {}", resp.status()));
        }
        let body: Value = resp.json().context("unexpected list_projects response")?;
        let projects: Vec<ProjectInfo> = serde_json::from_value(
            body["projects"].clone(),
        )
        .context("failed to parse projects list")?;
        Ok(projects)
    }

    pub fn get_project_by_slug(&self, slug: &str) -> Result<Option<ProjectInfo>> {
        let projects = self.list_projects()?;
        Ok(projects.into_iter().find(|p| p.slug == slug))
    }

    pub fn create_project(
        &self,
        token: &str,
        name: &str,
        slug: &str,
        description: Option<&str>,
    ) -> Result<ProjectInfo> {
        #[derive(Serialize)]
        struct Req<'a> {
            name: &'a str,
            slug: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            description: Option<&'a str>,
        }
        let url = format!("{}/projects", self.hub);
        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .json(&Req { name, slug, description })
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("create_project failed: {}", body));
        }
        resp.json::<ProjectInfo>().context("unexpected create_project response")
    }

    pub fn delete_project_rest(&self, token: &str, project_id: &str) -> Result<()> {
        let url = format!("{}/projects/{}", self.hub, project_id);
        let resp = self
            .client
            .delete(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("delete_project failed: {}", body));
        }
        Ok(())
    }

    pub fn get_protocol(&self, project_id: &str) -> Result<Option<String>> {
        let url = format!("{}/projects/{}/protocol", self.hub, project_id);
        let resp = self.client.get(&url).send().context("cannot reach hub")?;
        if resp.status().as_u16() == 404 || resp.status().as_u16() == 204 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(anyhow!("get_protocol failed: HTTP {}", resp.status()));
        }
        Ok(Some(resp.text().context("failed to read protocol text")?))
    }

    pub fn set_protocol(&self, token: &str, project_id: &str, text: &str) -> Result<()> {
        let url = format!("{}/projects/{}/protocol", self.hub, project_id);
        let resp = self
            .client
            .put(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .header("Content-Type", "text/plain")
            .body(text.to_string())
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("set_protocol failed: {}", body));
        }
        Ok(())
    }

    pub fn get_requirements(&self, project_id: &str) -> Result<Value> {
        let url = format!("{}/projects/{}/requirements", self.hub, project_id);
        let resp = self.client.get(&url).send().context("cannot reach hub")?;
        if !resp.status().is_success() {
            return Err(anyhow!("get_requirements failed: HTTP {}", resp.status()));
        }
        let body: Value = resp.json().context("unexpected get_requirements response")?;
        Ok(body["requirements"].clone())
    }

    pub fn set_requirements(&self, token: &str, project_id: &str, reqs: &Value) -> Result<()> {
        let url = format!("{}/projects/{}/requirements", self.hub, project_id);
        let resp = self
            .client
            .put(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .json(&json!({ "requirements": reqs }))
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("set_requirements failed: {}", body));
        }
        Ok(())
    }

    pub fn get_seed_bounty(&self, project_id: &str) -> Result<Option<Value>> {
        let url = format!("{}/projects/{}/seed-bounty", self.hub, project_id);
        let resp = self.client.get(&url).send().context("cannot reach hub")?;
        if resp.status().as_u16() == 404 || resp.status().as_u16() == 204 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(anyhow!("get_seed_bounty failed: HTTP {}", resp.status()));
        }
        let body: Value = resp.json().context("unexpected get_seed_bounty response")?;
        Ok(Some(body["seed_bounty"].clone()))
    }

    pub fn set_seed_bounty(&self, token: &str, project_id: &str, bounty: &Value) -> Result<()> {
        let url = format!("{}/projects/{}/seed-bounty", self.hub, project_id);
        let resp = self
            .client
            .put(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .json(&json!({ "seed_bounty": bounty }))
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("set_seed_bounty failed: {}", body));
        }
        Ok(())
    }

    pub fn list_project_files(&self, project_id: &str) -> Result<Vec<ProjectFileInfo>> {
        let url = format!("{}/projects/{}/files", self.hub, project_id);
        let resp = self.client.get(&url).send().context("cannot reach hub")?;
        if !resp.status().is_success() {
            return Err(anyhow!("list_project_files failed: HTTP {}", resp.status()));
        }
        let body: Value = resp.json().context("unexpected list_project_files response")?;
        let files: Vec<ProjectFileInfo> =
            serde_json::from_value(body["files"].clone()).context("failed to parse files list")?;
        Ok(files)
    }

    pub fn upload_project_file(
        &self,
        token: &str,
        project_id: &str,
        filename: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<()> {
        // Use a longer timeout for potentially large files
        let upload_client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("failed to build upload client")?;
        let url = format!("{}/projects/{}/files/{}", self.hub, project_id, filename);
        let resp = upload_client
            .put(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .header("Content-Type", content_type)
            .body(data)
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("upload_file failed: {}", body));
        }
        Ok(())
    }

    pub fn download_project_file(&self, project_id: &str, filename: &str) -> Result<Vec<u8>> {
        let url = format!("{}/projects/{}/files/{}", self.hub, project_id, filename);
        let resp = self.client.get(&url).send().context("cannot reach hub")?;
        if !resp.status().is_success() {
            return Err(anyhow!("download_file failed: HTTP {}", resp.status()));
        }
        Ok(resp.bytes().context("failed to read file bytes")?.to_vec())
    }

    pub fn delete_project_file(
        &self,
        token: &str,
        project_id: &str,
        filename: &str,
    ) -> Result<()> {
        let url = format!("{}/projects/{}/files/{}", self.hub, project_id, filename);
        let resp = self
            .client
            .delete(&url)
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .send()
            .context("cannot reach hub")?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("delete_file failed: {}", body));
        }
        Ok(())
    }
}
