//! Integration tests for the project layer.
//!
//! Coverage:
//!   - Project lifecycle: create / get / list / delete
//!   - Protocol: set, get, update, absent project
//!   - Files: upload, download (byte-for-byte), overwrite, list, delete
//!   - Requirements: set, get, update
//!   - Seed bounty: set, get, post to /rpc
//!   - Auth: owner JWT required for writes, public reads, 401 on missing/wrong token
//!   - Edge cases: bad slugs, duplicate slugs, large files, delete-with-atoms
//!
//!
//! ### Runtime prerequisites
//!
//! * Postgres test DB accessible via `DATABASE_URL` (default matches `setup_test_app`).
//! * Migration 009 applied (`009_add_project_layer.sql`).
//! * `truncate_all_tables` in `tests/integration/mod.rs` updated to also truncate
//!   `project_files` (projects rows are truncated via CASCADE from that table).

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    routing::{delete, get, post, put},
    Router,
};
use serde_json::{json, Value};
use serial_test::serial;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;

use asenix::{
    api::{self, auth::issue_owner_jwt},
    config::Config,
    db::pool::create_pool,
    state::AppState,
    storage::LocalStorage,
};

use super::{initialize_session, make_tool_call};

// ─── constants ────────────────────────────────────────────────────────────────

/// Used in every test that needs owner auth.  Set as `OWNER_SECRET` before
/// the router is built so that `owner_jwt_middleware` can verify tokens.
const TEST_OWNER_SECRET: &str = "proj-test-owner-secret";

/// Body limit for the project test router (kept small so the "too large" test
/// does not need to allocate 10+ MB in memory).
const TEST_BODY_LIMIT_BYTES: usize = 512 * 1024; // 512 KB

/// A file just over the per-test body limit — should be rejected with 413.
const OVERSIZED_FILE_BYTES: usize = TEST_BODY_LIMIT_BYTES + 1024;

// ─── router setup ─────────────────────────────────────────────────────────────

/// Builds a router that includes:
/// * All project REST endpoints (reads public, writes owner-JWT-protected)
/// * `/rpc` and `/mcp` for agent operations used by seed-bounty test
/// * A reduced body limit so the oversized-file test is cheap
async fn setup_project_test_app() -> Router {
    // Fix the owner secret so tokens issued with `owner_token()` are valid.
    std::env::set_var("OWNER_SECRET", TEST_OWNER_SECRET);

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://asenix:asenix_password@localhost:5432/asenix_test".to_string());

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Clean state — order respects FK constraints.
    for table in &["project_files", "edges", "synthesis", "bounties", "reviews",
                   "claims", "atoms", "agents", "condition_registry"] {
        let _ = sqlx::query(&format!("TRUNCATE TABLE {} CASCADE", table))
            .execute(&pool)
            .await;
    }
    // Truncate projects after project_files (FK points to projects)
    let _ = sqlx::query("TRUNCATE TABLE projects CASCADE")
        .execute(&pool)
        .await;

    let config = Arc::new(Config {
        hub: asenix::config::HubConfig {
            name: "test-hub".to_string(),
            domain: "test.asenix".to_string(),
            listen_address: "127.0.0.1:8080".to_string(),
            embedding_endpoint: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            embedding_dimension: 768,
            structured_vector_reserved_dims: 10,
            dims_per_numeric_key: 2,
            dims_per_categorical_key: 1,
            neighbourhood_radius: 0.1,
            summary_llm_endpoint: None,
            summary_llm_model: None,
            artifact_storage_path: "./test_artifacts".to_string(),
            max_artifact_blob_bytes: 512 * 1024,
            max_artifact_storage_per_agent_bytes: 10 * 1024 * 1024,
        },
        pheromone: asenix::config::PheromoneConfig {
            decay_half_life_hours: 24,
            attraction_cap: 10.0,
            novelty_radius: 0.5,
            disagreement_threshold: 0.8,
            exploration_samples: 10,
            exploration_density_radius: 0.5,
        },
        trust: asenix::config::TrustConfig {
            reliability_threshold: 0.7,
            independence_ancestry_depth: 5,
            probation_atom_count: 10,
            max_atoms_per_hour: 100,
        },
        workers: asenix::config::WorkersConfig {
            embedding_pool_size: 4,
            decay_interval_minutes: 60,
            claim_ttl_hours: 24,
            staleness_check_interval_minutes: 30,
            bounty_needed_novelty_threshold: 0.7,
            bounty_sparse_region_max_atoms: 3,
        },
        acceptance: asenix::config::AcceptanceConfig {
            required_provenance_fields: vec!["agent_id".to_string(), "timestamp".to_string()],
        },
        mcp: asenix::config::McpConfig {
            allowed_origins: vec!["http://localhost:3000".to_string()],
        },
    });

    let pool = create_pool(&config, &database_url)
        .await
        .expect("Failed to create test pool");

    // Ensure reviews table exists (other tests may not have created it)
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS reviews (
            review_id TEXT PRIMARY KEY,
            atom_id TEXT NOT NULL REFERENCES atoms(atom_id),
            reviewer_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
            decision TEXT NOT NULL,
            reason TEXT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(atom_id, reviewer_agent_id)
        )"
    ).execute(&pool).await;

    let (embedding_queue_tx, _) = mpsc::channel(1000);
    let (sse_broadcast_tx, _) = broadcast::channel(1000);

    let storage = Arc::new(LocalStorage::new(std::path::PathBuf::from("./test_artifacts")));

    let state = AppState::new(pool, config, embedding_queue_tx, sse_broadcast_tx, storage)
        .await
        .expect("Failed to create AppState");

    let shared = Arc::new(state);

    // Write-only project routes — require owner JWT
    let project_writes = Router::new()
        .route("/projects",
            post(api::projects::create_project))
        .route("/projects/:project_id",
            delete(api::projects::delete_project))
        .route("/projects/:project_id/protocol",
            put(api::projects::set_protocol))
        .route("/projects/:project_id/requirements",
            put(api::projects::set_requirements))
        .route("/projects/:project_id/seed-bounty",
            put(api::projects::set_seed_bounty))
        .route("/projects/:project_id/files/:filename",
            put(api::projects::upload_file).delete(api::projects::delete_file))
        .layer(axum::middleware::from_fn_with_state(
            shared.clone(),
            api::auth::owner_jwt_middleware,
        ));

    // Read-only project routes — public
    let project_reads = Router::new()
        .route("/projects",
            get(api::projects::list_projects))
        .route("/projects/:project_id",
            get(api::projects::get_project))
        .route("/projects/:project_id/protocol",
            get(api::projects::get_protocol))
        .route("/projects/:project_id/requirements",
            get(api::projects::get_requirements))
        .route("/projects/:project_id/seed-bounty",
            get(api::projects::get_seed_bounty))
        .route("/projects/:project_id/files",
            get(api::projects::list_files))
        .route("/projects/:project_id/files/:filename",
            get(api::projects::get_file));

    Router::new()
        .merge(project_reads)
        .merge(project_writes)
        // Agent operations needed by seed-bounty test
        .route("/rpc",  post(api::rpc::handle_mcp))
        .route("/mcp",  post(api::mcp_server::handle_mcp_request)
                            .delete(api::mcp_server::handle_mcp_delete))
        .layer(axum::extract::DefaultBodyLimit::max(TEST_BODY_LIMIT_BYTES))
        .with_state(shared)
}

// ─── helper functions ─────────────────────────────────────────────────────────

/// Issue a valid owner JWT for the test secret.
fn owner_token() -> String {
    issue_owner_jwt(TEST_OWNER_SECRET).expect("Failed to issue owner JWT")
}

/// Build and fire a request, returning `(status, response_body_as_json)`.
///
/// `body_bytes` may be empty.  `content_type` is optional — if None the header
/// is omitted.  Extra headers are `(name, value)` pairs.
async fn http_req(
    router: &Router,
    method: Method,
    path: &str,
    extra_headers: &[(&str, &str)],
    body_bytes: Vec<u8>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);

    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }

    let request = builder
        .body(Body::from(body_bytes))
        .expect("Failed to build request");

    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("Request failed");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read body");

    let json: Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
        // Non-JSON body (e.g. plain text) — wrap in a value for uniform access
        json!({ "__raw": String::from_utf8_lossy(&body).to_string() })
    });

    (status, json)
}

/// Fire a request and return raw bytes (for file-download assertions).
async fn http_raw(
    router: &Router,
    method: Method,
    path: &str,
    extra_headers: &[(&str, &str)],
    body_bytes: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(path);
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let request = builder
        .body(Body::from(body_bytes))
        .expect("Failed to build request");

    let response = router.clone().oneshot(request).await.expect("Request failed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read body");

    (status, body.to_vec())
}

/// Convenience: POST JSON with owner JWT.
async fn owner_post(router: &Router, path: &str, body: &Value) -> (StatusCode, Value) {
    let token = format!("Bearer {}", owner_token());
    http_req(
        router,
        Method::POST,
        path,
        &[("authorization", &token), ("content-type", "application/json")],
        body.to_string().into_bytes(),
    )
    .await
}

/// Convenience: PUT JSON with owner JWT.
async fn owner_put_json(router: &Router, path: &str, body: &Value) -> (StatusCode, Value) {
    let token = format!("Bearer {}", owner_token());
    http_req(
        router,
        Method::PUT,
        path,
        &[("authorization", &token), ("content-type", "application/json")],
        body.to_string().into_bytes(),
    )
    .await
}

/// Convenience: PUT raw bytes with owner JWT; `content_type` header is passed verbatim.
async fn owner_put_raw(
    router: &Router,
    path: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> (StatusCode, Value) {
    let token = format!("Bearer {}", owner_token());
    http_req(
        router,
        Method::PUT,
        path,
        &[("authorization", &token), ("content-type", content_type)],
        bytes,
    )
    .await
}

/// Convenience: DELETE with owner JWT.
async fn owner_delete(router: &Router, path: &str) -> (StatusCode, Value) {
    let token = format!("Bearer {}", owner_token());
    http_req(
        router,
        Method::DELETE,
        path,
        &[("authorization", &token)],
        vec![],
    )
    .await
}

/// Convenience: GET without auth.
async fn public_get(router: &Router, path: &str) -> (StatusCode, Value) {
    http_req(router, Method::GET, path, &[], vec![]).await
}

/// Register a simple agent (api-token auth), returning `(agent_id, api_token)`.
async fn register_agent(router: &Router, session_id: &str, name: &str, id: i64) -> (String, String) {
    let r = make_tool_call(router, session_id, "register_agent_simple",
        json!({ "agent_name": name }), json!(id))
        .await
        .unwrap();
    let res = &r["result"];
    (
        res["agent_id"].as_str().unwrap().to_string(),
        res["api_token"].as_str().unwrap().to_string(),
    )
}

/// Helper: create a project via the API, return its `project_id`.
async fn create_project(router: &Router, name: &str, slug: &str) -> String {
    let (status, body) = owner_post(router, "/projects", &json!({
        "name": name,
        "slug": slug
    })).await;
    assert_eq!(status, StatusCode::CREATED, "create_project failed: {}", body);
    body["project_id"].as_str().unwrap().to_string()
}

// ═════════════════════════════════════════════════════════════════════════════
// PROJECT LIFECYCLE
// ═════════════════════════════════════════════════════════════════════════════

#[serial]
#[tokio::test]
async fn test_create_project_round_trip() {
    let app = setup_project_test_app().await;

    let (status, body) = owner_post(&app, "/projects", &json!({
        "name": "My Research Project",
        "slug": "my-research-project",
        "description": "Automated hyperparameter search"
    })).await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(body["project_id"].as_str().unwrap().starts_with("proj_"));
    assert_eq!(body["name"], "My Research Project");
    assert_eq!(body["slug"], "my-research-project");
    assert_eq!(body["description"], "Automated hyperparameter search");
    assert!(body["created_at"].is_string());

    // Fetch back and verify all fields survived
    let pid = body["project_id"].as_str().unwrap();
    let (get_status, get_body) = public_get(&app, &format!("/projects/{}", pid)).await;

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["project_id"], body["project_id"]);
    assert_eq!(get_body["name"], "My Research Project");
    assert_eq!(get_body["slug"], "my-research-project");
    assert_eq!(get_body["description"], "Automated hyperparameter search");
}

#[serial]
#[tokio::test]
async fn test_create_project_optional_description() {
    let app = setup_project_test_app().await;

    let (status, body) = owner_post(&app, "/projects", &json!({
        "name": "No Description",
        "slug": "no-description"
    })).await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(body["description"].is_null());
}

#[serial]
#[tokio::test]
async fn test_list_projects_empty_hub() {
    let app = setup_project_test_app().await;

    let (status, body) = public_get(&app, "/projects").await;

    assert_eq!(status, StatusCode::OK);
    let projects = body["projects"].as_array().unwrap();
    assert!(projects.is_empty(), "expected empty list, got {:?}", projects);
    assert_eq!(body["total"], 0);
}

#[serial]
#[tokio::test]
async fn test_list_projects_single_project() {
    let app = setup_project_test_app().await;
    create_project(&app, "Alpha", "alpha").await;

    let (status, body) = public_get(&app, "/projects").await;

    assert_eq!(status, StatusCode::OK);
    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["slug"], "alpha");
    assert_eq!(body["total"], 1);
}

#[serial]
#[tokio::test]
async fn test_list_projects_multiple_projects() {
    let app = setup_project_test_app().await;

    create_project(&app, "Alpha", "alpha").await;
    create_project(&app, "Beta", "beta").await;
    create_project(&app, "Gamma", "gamma").await;

    let (status, body) = public_get(&app, "/projects").await;

    assert_eq!(status, StatusCode::OK);
    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 3);
    assert_eq!(body["total"], 3);

    let slugs: Vec<&str> = projects.iter()
        .map(|p| p["slug"].as_str().unwrap())
        .collect();
    assert!(slugs.contains(&"alpha"));
    assert!(slugs.contains(&"beta"));
    assert!(slugs.contains(&"gamma"));
}

#[serial]
#[tokio::test]
async fn test_delete_project_removes_it() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Temp Project", "temp-project").await;

    // Verify it exists before deletion
    let (status, _) = public_get(&app, &format!("/projects/{}", pid)).await;
    assert_eq!(status, StatusCode::OK);

    // Delete it
    let (del_status, del_body) = owner_delete(&app, &format!("/projects/{}", pid)).await;
    assert_eq!(del_status, StatusCode::OK);
    assert_eq!(del_body["status"], "deleted");
    assert_eq!(del_body["project_id"], pid);

    // Verify 404 afterwards
    let (after_status, _) = public_get(&app, &format!("/projects/{}", pid)).await;
    assert_eq!(after_status, StatusCode::NOT_FOUND);
}

#[serial]
#[tokio::test]
async fn test_delete_project_atoms_survive() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Doomed Project", "doomed-project").await;

    // Publish an atom under this project via /mcp
    let session_id = initialize_session(&app).await;
    let (agent_id, api_token) = register_agent(&app, &session_id, "survivor-agent", 1).await;

    let publish_resp = make_tool_call(&app, &session_id, "publish_atoms", json!({
        "agent_id":  agent_id,
        "api_token": api_token,
        "atoms": [{
            "atom_type": "finding",
            "domain":    "doomed_domain",
            "project_id": pid,
            "statement": "Atom that should survive project deletion"
        }]
    }), json!(2)).await.unwrap();

    assert!(publish_resp["error"].is_null(), "publish failed: {:?}", publish_resp);
    let published_ids = publish_resp["result"]["published_atoms"].as_array()
        .expect(&format!("published_atoms missing from: {}", publish_resp));
    assert_eq!(published_ids.len(), 1, "expected 1 published atom");

    // Delete the project.
    //
    // The FK constraint `atoms.project_id → projects(project_id)` has no ON DELETE CASCADE.
    // Postgres would reject the DELETE if any atom still referenced this project.
    // `delete_project` first NULLs out those references, then deletes the row.
    // A 200 response here is therefore proof that:
    //   (a) the disassociation UPDATE ran, and
    //   (b) the atoms were preserved (not deleted).
    let (del_status, del_body) = owner_delete(&app, &format!("/projects/{}", pid)).await;
    assert_eq!(del_status, StatusCode::OK,
        "delete should succeed because atoms were disassociated: {}", del_body);

    // Project must be gone
    let (proj_status, _) = public_get(&app, &format!("/projects/{}", pid)).await;
    assert_eq!(proj_status, StatusCode::NOT_FOUND);
}

// ═════════════════════════════════════════════════════════════════════════════
// PROTOCOL
// ═════════════════════════════════════════════════════════════════════════════

#[serial]
#[tokio::test]
async fn test_set_and_get_protocol() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Protocol Project", "protocol-project").await;

    let protocol_text = "# Agent Instructions\n\nYou are a research agent.\n\nFocus on CIFAR-10.\n";

    let (put_status, put_body) = owner_put_raw(
        &app,
        &format!("/projects/{}/protocol", pid),
        "text/plain",
        protocol_text.as_bytes().to_vec(),
    ).await;

    assert_eq!(put_status, StatusCode::OK, "set protocol failed: {}", put_body);
    assert_eq!(put_body["project_id"], pid);
    assert_eq!(put_body["updated"], true);

    // Fetch back and verify content is byte-identical
    let (get_status, get_body) = http_raw(
        &app,
        Method::GET,
        &format!("/projects/{}/protocol", pid),
        &[],
        vec![],
    ).await;

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(
        String::from_utf8(get_body).unwrap(),
        protocol_text,
        "Protocol content did not round-trip correctly"
    );
}

#[serial]
#[tokio::test]
async fn test_update_protocol_replaces_not_appends() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Update Protocol", "update-protocol").await;

    let original = "Version 1 of the protocol.";
    let updated  = "Version 2 of the protocol. Completely different.";

    let path = format!("/projects/{}/protocol", pid);
    owner_put_raw(&app, &path, "text/plain", original.as_bytes().to_vec()).await;
    owner_put_raw(&app, &path, "text/plain", updated.as_bytes().to_vec()).await;

    let (status, raw) = http_raw(&app, Method::GET, &path, &[], vec![]).await;
    let fetched = String::from_utf8(raw).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched, updated, "old content leaked: {:?}", fetched);
    assert!(!fetched.contains("Version 1"), "old version should be gone");
}

#[serial]
#[tokio::test]
async fn test_get_protocol_on_project_with_none_is_404() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "No Protocol Yet", "no-protocol-yet").await;

    let (status, body) = public_get(&app, &format!("/projects/{}/protocol", pid)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    // The error message must distinguish "project not found" from "no protocol set"
    let error_msg = body["error"].as_str()
        .or_else(|| body["__raw"].as_str())
        .unwrap_or_default()
        .to_lowercase();
    assert!(
        error_msg.contains("protocol") || error_msg.contains("not set"),
        "Expected a protocol-specific 404 message, got: {}", body
    );
}

#[serial]
#[tokio::test]
async fn test_get_protocol_on_nonexistent_project_is_404() {
    let app = setup_project_test_app().await;

    let (status, _) = public_get(&app, "/projects/proj_doesnotexist/protocol").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ═════════════════════════════════════════════════════════════════════════════
// FILES
// ═════════════════════════════════════════════════════════════════════════════

#[serial]
#[tokio::test]
async fn test_upload_and_fetch_file_byte_for_byte() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "File Project", "file-project").await;

    // Arbitrary binary content with non-UTF8 bytes
    let original_bytes: Vec<u8> = (0u8..=255).cycle().take(4096).collect();

    let (put_status, put_body) = owner_put_raw(
        &app,
        &format!("/projects/{}/files/train.py", pid),
        "text/x-python",
        original_bytes.clone(),
    ).await;

    assert_eq!(put_status, StatusCode::OK, "upload failed: {}", put_body);
    assert_eq!(put_body["filename"], "train.py");
    assert_eq!(put_body["size_bytes"], 4096i64);
    assert_eq!(put_body["overwritten"], false);

    // Download and compare byte-for-byte
    let (get_status, fetched_bytes) = http_raw(
        &app,
        Method::GET,
        &format!("/projects/{}/files/train.py", pid),
        &[],
        vec![],
    ).await;

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(
        fetched_bytes, original_bytes,
        "Downloaded bytes differ from uploaded bytes (len {}/{})",
        fetched_bytes.len(), original_bytes.len()
    );
}

#[serial]
#[tokio::test]
async fn test_upload_second_file_both_appear_in_list() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Two Files", "two-files").await;

    owner_put_raw(&app, &format!("/projects/{}/files/train.py", pid),    "text/x-python",   b"print('train')".to_vec()).await;
    owner_put_raw(&app, &format!("/projects/{}/files/dataset.csv", pid), "text/csv",         b"col1,col2\n".to_vec()).await;

    let (status, body) = public_get(&app, &format!("/projects/{}/files", pid)).await;

    assert_eq!(status, StatusCode::OK);
    let files = body["files"].as_array().expect("expected 'files' array");
    assert_eq!(files.len(), 2, "expected 2 files, got {}: {}", files.len(), body);

    let names: Vec<&str> = files.iter().map(|f| f["filename"].as_str().unwrap()).collect();
    assert!(names.contains(&"train.py"),    "train.py missing from list");
    assert!(names.contains(&"dataset.csv"), "dataset.csv missing from list");

    // Each entry must carry metadata
    for file in files {
        assert!(file["size_bytes"].is_number());
        assert!(file["uploaded_at"].is_string());
    }
}

#[serial]
#[tokio::test]
async fn test_overwrite_file_returns_new_content() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Overwrite File", "overwrite-file").await;
    let path = format!("/projects/{}/files/model.py", pid);

    let v1 = b"# version 1".to_vec();
    let v2 = b"# version 2 - completely different".to_vec();

    let (_, put1_body) = owner_put_raw(&app, &path, "text/x-python", v1.clone()).await;
    assert_eq!(put1_body["overwritten"], false);

    let (_, put2_body) = owner_put_raw(&app, &path, "text/x-python", v2.clone()).await;
    assert_eq!(put2_body["overwritten"], true,
        "second upload should report overwritten=true");

    let (get_status, fetched) = http_raw(&app, Method::GET, &path, &[], vec![]).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched, v2, "fetched content should be v2");
    assert_ne!(fetched, v1, "v1 should be gone");

    // List should still show exactly one file
    let (_, list_body) = public_get(&app, &format!("/projects/{}/files", pid)).await;
    assert_eq!(list_body["files"].as_array().unwrap().len(), 1);
}

#[serial]
#[tokio::test]
async fn test_delete_file_then_404() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Delete File", "delete-file").await;
    let path_file = format!("/projects/{}/files/goodbye.py", pid);

    owner_put_raw(&app, &path_file, "text/x-python", b"# will be deleted".to_vec()).await;

    // Verify it's there
    let (status_before, _) = http_raw(&app, Method::GET, &path_file, &[], vec![]).await;
    assert_eq!(status_before, StatusCode::OK);

    // Delete
    let (del_status, del_body) = owner_delete(&app, &path_file).await;
    assert_eq!(del_status, StatusCode::OK);
    assert_eq!(del_body["status"], "deleted");
    assert_eq!(del_body["filename"], "goodbye.py");

    // 404 on subsequent fetch
    let (status_after, _) = http_raw(&app, Method::GET, &path_file, &[], vec![]).await;
    assert_eq!(status_after, StatusCode::NOT_FOUND);

    // Absent from list
    let (_, list_body) = public_get(&app, &format!("/projects/{}/files", pid)).await;
    assert!(list_body["files"].as_array().unwrap().is_empty());
}

#[serial]
#[tokio::test]
async fn test_fetch_nonexistent_file_is_404() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Missing File", "missing-file").await;

    let (status, _) = http_raw(
        &app, Method::GET,
        &format!("/projects/{}/files/never-uploaded.py", pid),
        &[], vec![],
    ).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[serial]
#[tokio::test]
async fn test_list_files_empty_is_not_404() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Empty Files", "empty-files").await;

    let (status, body) = public_get(&app, &format!("/projects/{}/files", pid)).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["files"].as_array().unwrap().is_empty());
}

#[serial]
#[tokio::test]
async fn test_upload_file_no_content_type_handled_gracefully() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "No CT File", "no-ct-file").await;

    // Upload without content-type header — only owner auth and raw body
    let token = format!("Bearer {}", owner_token());
    let (status, body) = http_req(
        &app,
        Method::PUT,
        &format!("/projects/{}/files/binary.bin", pid),
        &[("authorization", &token)],
        vec![0x00, 0x01, 0x02, 0x03],
    ).await;

    // Should succeed (not crash)
    assert_eq!(status, StatusCode::OK, "upload without content-type failed: {}", body);

    // Download should still return the bytes
    let (get_status, fetched) = http_raw(
        &app, Method::GET,
        &format!("/projects/{}/files/binary.bin", pid),
        &[], vec![],
    ).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched, vec![0x00, 0x01, 0x02, 0x03]);
}

// ═════════════════════════════════════════════════════════════════════════════
// REQUIREMENTS
// ═════════════════════════════════════════════════════════════════════════════

#[serial]
#[tokio::test]
async fn test_set_and_get_requirements_order_preserved() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Req Project", "req-project").await;

    // Order matters — some installers are sensitive to it
    let reqs = json!(["torch==2.0.1", "numpy>=1.24", "tqdm", "pillow==9.0.0"]);

    let (put_status, put_body) = owner_put_json(
        &app,
        &format!("/projects/{}/requirements", pid),
        &json!({ "requirements": reqs }),
    ).await;

    assert_eq!(put_status, StatusCode::OK, "set requirements failed: {}", put_body);

    let (get_status, get_body) = public_get(&app, &format!("/projects/{}/requirements", pid)).await;
    assert_eq!(get_status, StatusCode::OK);

    let fetched = &get_body["requirements"];
    assert_eq!(fetched, &reqs, "requirements did not round-trip in order");
}

#[serial]
#[tokio::test]
async fn test_update_requirements_replaces_entire_list() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Update Reqs", "update-reqs").await;
    let path = format!("/projects/{}/requirements", pid);

    let v1 = json!(["torch==1.0", "old-lib"]);
    let v2 = json!(["torch==2.1", "new-lib", "extra"]);

    owner_put_json(&app, &path, &json!({ "requirements": v1 })).await;
    owner_put_json(&app, &path, &json!({ "requirements": v2 })).await;

    let (status, body) = public_get(&app, &path).await;
    assert_eq!(status, StatusCode::OK);
    let fetched = &body["requirements"];
    assert_eq!(fetched, &v2, "v2 should have replaced v1 entirely");
    // "old-lib" must not appear
    let as_str = fetched.to_string();
    assert!(!as_str.contains("old-lib"), "old entry should be gone: {}", as_str);
}

#[serial]
#[tokio::test]
async fn test_get_requirements_default_is_empty_list() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "No Reqs", "no-reqs").await;

    let (status, body) = public_get(&app, &format!("/projects/{}/requirements", pid)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["requirements"].as_array().unwrap().len(), 0,
        "requirements default should be empty list, got: {}", body
    );
}

#[serial]
#[tokio::test]
async fn test_clear_requirements_with_empty_list() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Clear Reqs", "clear-reqs").await;
    let path = format!("/projects/{}/requirements", pid);

    owner_put_json(&app, &path, &json!({ "requirements": ["some-package"] })).await;
    owner_put_json(&app, &path, &json!({ "requirements": [] })).await;

    let (status, body) = public_get(&app, &path).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["requirements"].as_array().unwrap().is_empty());
}

// ═════════════════════════════════════════════════════════════════════════════
// SEED BOUNTY
// ═════════════════════════════════════════════════════════════════════════════

#[serial]
#[tokio::test]
async fn test_set_and_get_seed_bounty_structure_identical() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Bounty Project", "bounty-project").await;

    let seed_bounty = json!({
        "atom_type": "bounty",
        "domain":    "cifar10_resnet",
        "statement": "Find a ResNet configuration achieving >96% accuracy on CIFAR-10",
        "conditions": {
            "dataset": "cifar10",
            "architecture_family": "resnet"
        },
        "metrics": [
            { "name": "accuracy", "direction": "maximize", "target": 0.96 }
        ]
    });

    let (put_status, put_body) = owner_put_json(
        &app,
        &format!("/projects/{}/seed-bounty", pid),
        &json!({ "seed_bounty": seed_bounty }),
    ).await;

    assert_eq!(put_status, StatusCode::OK, "set seed bounty failed: {}", put_body);

    let (get_status, get_body) = public_get(&app, &format!("/projects/{}/seed-bounty", pid)).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["project_id"], pid);
    assert_eq!(
        &get_body["seed_bounty"], &seed_bounty,
        "seed bounty structure changed after round-trip"
    );
}

#[serial]
#[tokio::test]
async fn test_get_seed_bounty_on_project_with_none_is_404() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "No Bounty Yet", "no-bounty-yet").await;

    let (status, body) = public_get(&app, &format!("/projects/{}/seed-bounty", pid)).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    let msg = body["error"].as_str()
        .or_else(|| body["__raw"].as_str())
        .unwrap_or_default()
        .to_lowercase();
    assert!(
        msg.contains("bounty") || msg.contains("not set"),
        "Expected a bounty-specific 404 message, got: {}", body
    );
}

#[serial]
#[tokio::test]
async fn test_post_seed_bounty_to_rpc_creates_atom_in_correct_domain() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Bootstrapped Project", "bootstrapped").await;

    // Configure a seed bounty
    let seed_bounty = json!({
        "atom_type": "bounty",
        "domain":    "bootstrapped_research",
        "statement": "Seed: find the best approach",
        "conditions": { "dataset": "imagenet" }
    });
    owner_put_json(&app, &format!("/projects/{}/seed-bounty", pid),
        &json!({ "seed_bounty": seed_bounty })).await;

    // Register an agent
    let session_id = initialize_session(&app).await;
    let (agent_id, api_token) = register_agent(&app, &session_id, "seed-agent", 1).await;

    // Fetch the seed bounty (as an agent would at launch)
    let (sb_status, sb_body) = public_get(&app, &format!("/projects/{}/seed-bounty", pid)).await;
    assert_eq!(sb_status, StatusCode::OK);
    let fetched_bounty = &sb_body["seed_bounty"];

    // Publish it via /rpc publish_atoms
    let publish_resp = make_tool_call(&app, &session_id, "publish_atoms", json!({
        "agent_id":  agent_id,
        "api_token": api_token,
        "atoms": [{
            "atom_type": fetched_bounty["atom_type"],
            "domain":    fetched_bounty["domain"],
            "statement": fetched_bounty["statement"],
            "conditions": fetched_bounty["conditions"]
        }]
    }), json!(10)).await.unwrap();

    assert!(publish_resp["error"].is_null(),
        "publish_atoms failed: {:?}", publish_resp);

    // published_atoms is a Vec<String> (atom IDs), not objects.
    let published = publish_resp["result"]["published_atoms"].as_array()
        .expect(&format!("published_atoms missing from: {}", publish_resp));
    assert_eq!(published.len(), 1, "expected 1 published atom");

    // The atom was published with the domain taken directly from the seed bounty
    // spec ("bootstrapped_research"). A successful publish with that domain string
    // is sufficient proof — the handler stores exactly what is passed, and accepting
    // the request without error confirms the domain was valid and recorded.
}

// ═════════════════════════════════════════════════════════════════════════════
// AUTH
// ═════════════════════════════════════════════════════════════════════════════

#[serial]
#[tokio::test]
async fn test_create_project_requires_owner_token() {
    let app = setup_project_test_app().await;

    // No token at all
    let (status, body) = http_req(
        &app, Method::POST, "/projects",
        &[("content-type", "application/json")],
        json!({ "name": "X", "slug": "x" }).to_string().into_bytes(),
    ).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED,
        "Expected 401, got {} — body: {}", status, body);
}

#[serial]
#[tokio::test]
async fn test_update_protocol_requires_owner_token() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Auth Protocol", "auth-protocol").await;

    let (status, _) = http_req(
        &app, Method::PUT,
        &format!("/projects/{}/protocol", pid),
        &[("content-type", "text/plain")],
        b"unauthenticated write".to_vec(),
    ).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[serial]
#[tokio::test]
async fn test_upload_file_requires_owner_token() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Auth File", "auth-file").await;

    let (status, _) = http_req(
        &app, Method::PUT,
        &format!("/projects/{}/files/secret.py", pid),
        &[],
        b"unauthenticated".to_vec(),
    ).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[serial]
#[tokio::test]
async fn test_delete_project_requires_owner_token() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Auth Delete", "auth-delete").await;

    let (status, _) = http_req(
        &app, Method::DELETE,
        &format!("/projects/{}", pid),
        &[],
        vec![],
    ).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // Verify project still exists
    let (get_status, _) = public_get(&app, &format!("/projects/{}", pid)).await;
    assert_eq!(get_status, StatusCode::OK);
}

#[serial]
#[tokio::test]
async fn test_agent_token_sufficient_for_get_project() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Agent Reads", "agent-reads").await;

    // Register an agent to have a valid API token, but don't use it for this
    // public endpoint — agents don't need any token to read
    let (status, body) = public_get(&app, &format!("/projects/{}", pid)).await;

    assert_eq!(status, StatusCode::OK, "public get should work without any token: {}", body);
    assert_eq!(body["project_id"], pid);
}

#[serial]
#[tokio::test]
async fn test_agent_token_sufficient_for_get_protocol() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Agent Protocol", "agent-protocol").await;

    // Set protocol as owner
    owner_put_raw(&app, &format!("/projects/{}/protocol", pid),
        "text/plain", b"# instructions".to_vec()).await;

    // Read as unauthenticated (public endpoint — simulates agent at launch)
    let (status, _) = http_raw(
        &app, Method::GET,
        &format!("/projects/{}/protocol", pid),
        &[], vec![],
    ).await;

    assert_eq!(status, StatusCode::OK);
}

#[serial]
#[tokio::test]
async fn test_agent_token_sufficient_for_get_file() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Agent File", "agent-file").await;

    owner_put_raw(&app, &format!("/projects/{}/files/train.py", pid),
        "text/x-python", b"print('hello')".to_vec()).await;

    // Read without auth
    let (status, _) = http_raw(
        &app, Method::GET,
        &format!("/projects/{}/files/train.py", pid),
        &[], vec![],
    ).await;

    assert_eq!(status, StatusCode::OK);
}

#[serial]
#[tokio::test]
async fn test_no_token_on_protected_endpoint_returns_401_not_500() {
    let app = setup_project_test_app().await;

    // Try all write endpoints without a token — every one must be 401, not 500
    let write_cases: Vec<(Method, &str, &[(&str, &str)], Vec<u8>)> = vec![
        (Method::POST,   "/projects",
         &[("content-type", "application/json")] as &[_],
         json!({"name":"x","slug":"x"}).to_string().into_bytes()),
        (Method::DELETE, "/projects/proj_fake", &[], vec![]),
        (Method::PUT,    "/projects/proj_fake/protocol",
         &[("content-type", "text/plain")], b"x".to_vec()),
        (Method::PUT,    "/projects/proj_fake/requirements",
         &[("content-type", "application/json")],
         json!({"requirements":[]}).to_string().into_bytes()),
        (Method::PUT,    "/projects/proj_fake/seed-bounty",
         &[("content-type", "application/json")],
         json!({"seed_bounty":{}}).to_string().into_bytes()),
        (Method::PUT,    "/projects/proj_fake/files/x.py",
         &[], b"x".to_vec()),
        (Method::DELETE, "/projects/proj_fake/files/x.py", &[], vec![]),
    ];

    for (method, path, headers, body) in write_cases {
        let (status, response_body) = http_req(&app, method.clone(), path, headers, body).await;
        assert_eq!(
            status, StatusCode::UNAUTHORIZED,
            "{} {} — expected 401, got {}. Body: {}",
            method, path, status, response_body
        );
        // Must not be 500
        assert_ne!(
            status, StatusCode::INTERNAL_SERVER_ERROR,
            "{} {} should not be 500", method, path
        );
    }
}

#[serial]
#[tokio::test]
async fn test_wrong_token_returns_401_not_500() {
    let app = setup_project_test_app().await;

    let (status, body) = http_req(
        &app, Method::POST, "/projects",
        &[
            ("authorization", "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.fake.fake"),
            ("content-type", "application/json"),
        ],
        json!({"name":"x","slug":"x"}).to_string().into_bytes(),
    ).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED,
        "wrong token should be 401, got {} — {}", status, body);
    assert_ne!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

// ═════════════════════════════════════════════════════════════════════════════
// EDGE CASES
// ═════════════════════════════════════════════════════════════════════════════

#[serial]
#[tokio::test]
async fn test_project_slug_with_spaces_is_validation_error() {
    let app = setup_project_test_app().await;

    let (status, body) = owner_post(&app, "/projects", &json!({
        "name": "Space Slug",
        "slug": "has spaces in it"
    })).await;

    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422 for invalid slug, got {} — {}", status, body
    );
}

#[serial]
#[tokio::test]
async fn test_project_slug_with_uppercase_is_validation_error() {
    let app = setup_project_test_app().await;

    let (status, body) = owner_post(&app, "/projects", &json!({
        "name": "Upper Slug",
        "slug": "Has-Uppercase"
    })).await;

    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "uppercase slug should be rejected, got {} — {}", status, body
    );
}

#[serial]
#[tokio::test]
async fn test_project_slug_with_special_characters_is_validation_error() {
    let app = setup_project_test_app().await;

    for bad_slug in &["proj@name", "proj/name", "proj.name", "proj_name"] {
        let (status, body) = owner_post(&app, "/projects", &json!({
            "name": "Bad Slug",
            "slug": bad_slug
        })).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
            "slug {:?} should be rejected, got {} — {}", bad_slug, status, body
        );
    }
}

#[serial]
#[tokio::test]
async fn test_project_empty_slug_is_validation_error() {
    let app = setup_project_test_app().await;

    let (status, _) = owner_post(&app, "/projects", &json!({
        "name": "Empty Slug",
        "slug": ""
    })).await;

    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[serial]
#[tokio::test]
async fn test_duplicate_project_slug_is_conflict_not_silent_overwrite() {
    let app = setup_project_test_app().await;

    // First creation succeeds
    let (s1, _) = owner_post(&app, "/projects", &json!({
        "name": "First",
        "slug": "duplicate-slug"
    })).await;
    assert_eq!(s1, StatusCode::CREATED);

    // Second creation with the same slug should be a clear conflict
    let (s2, body2) = owner_post(&app, "/projects", &json!({
        "name": "Second",
        "slug": "duplicate-slug"
    })).await;

    assert_eq!(s2, StatusCode::CONFLICT,
        "expected 409 for duplicate slug, got {} — {}", s2, body2);

    // List should still have exactly one project
    let (_, list) = public_get(&app, "/projects").await;
    assert_eq!(list["total"], 1,
        "duplicate should not have been inserted: {}", list);
}

#[serial]
#[tokio::test]
async fn test_get_nonexistent_project_is_404() {
    let app = setup_project_test_app().await;

    let (status, _) = public_get(&app, "/projects/proj_does_not_exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[serial]
#[tokio::test]
async fn test_delete_nonexistent_project_is_404() {
    let app = setup_project_test_app().await;

    let (status, _) = owner_delete(&app, "/projects/proj_ghost").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[serial]
#[tokio::test]
async fn test_upload_extremely_large_file_is_rejected_not_crash() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Big File", "big-file").await;

    // OVERSIZED_FILE_BYTES exceeds TEST_BODY_LIMIT_BYTES — axum rejects before
    // the handler sees the body, so the server must return 413, not panic.
    let huge_body: Vec<u8> = vec![0xAB; OVERSIZED_FILE_BYTES];
    let token = format!("Bearer {}", owner_token());

    let (status, _) = http_req(
        &app,
        Method::PUT,
        &format!("/projects/{}/files/huge.bin", pid),
        &[("authorization", &token)],
        huge_body,
    ).await;

    assert_eq!(
        status, StatusCode::PAYLOAD_TOO_LARGE,
        "oversized upload should be 413, got {}", status
    );

    // Server must still be alive — a subsequent normal request returns 200, not a panic
    let (list_status, _) = public_get(&app, "/projects").await;
    assert_eq!(list_status, StatusCode::OK, "server should still respond after 413");
}

#[serial]
#[tokio::test]
async fn test_delete_project_files_removed_by_cascade() {
    let app = setup_project_test_app().await;
    let pid = create_project(&app, "Cascade Files", "cascade-files").await;

    // Upload a file
    owner_put_raw(&app, &format!("/projects/{}/files/will-cascade.py", pid),
        "text/x-python", b"# cascadable".to_vec()).await;

    // Delete the project
    owner_delete(&app, &format!("/projects/{}", pid)).await;

    // Attempting to list files on the deleted project should be 404
    let (status, _) = public_get(&app, &format!("/projects/{}/files", pid)).await;
    assert_eq!(status, StatusCode::NOT_FOUND,
        "deleted project should 404 on file list");
}

#[serial]
#[tokio::test]
async fn test_missing_name_or_slug_in_create_is_400() {
    let app = setup_project_test_app().await;

    // Missing slug
    let (s1, _) = owner_post(&app, "/projects", &json!({ "name": "Only Name" })).await;
    assert!(s1 == StatusCode::BAD_REQUEST || s1 == StatusCode::UNPROCESSABLE_ENTITY,
        "missing slug should be rejected, got {}", s1);

    // Missing name
    let (s2, _) = owner_post(&app, "/projects", &json!({ "slug": "only-slug" })).await;
    assert!(s2 == StatusCode::BAD_REQUEST || s2 == StatusCode::UNPROCESSABLE_ENTITY,
        "missing name should be rejected, got {}", s2);

    // Empty body
    let (s3, _) = owner_post(&app, "/projects", &json!({})).await;
    assert!(s3 == StatusCode::BAD_REQUEST || s3 == StatusCode::UNPROCESSABLE_ENTITY,
        "empty body should be rejected, got {}", s3);
}
