#![allow(dead_code)]
#![allow(clippy::redundant_pattern_matching, clippy::should_implement_trait)]

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post, put, head};
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tower_http::cors::{CorsLayer, Any};

mod api;
mod config;
mod crypto;
mod db;
mod domain;
mod embedding;
mod error;
mod state;
mod metrics;
mod workers;
mod acceptance;
mod storage;

#[derive(Parser)]
#[command(name = "asenix")]
#[command(about = "Asenix coordination hub for asynchronous AI research agents")]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "asenix=debug,tower_http=debug,axum=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = config::Config::load_from_file(&args.config)?;
    config.validate()?;
    let config = std::sync::Arc::new(config);

    // Get database URL from environment or use default
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://asenix:asenix_password@localhost:5432/asenix".to_string());

    // Create database connection pool
    let pool = db::pool::create_pool(&config, &database_url).await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Create application state
    let (sse_broadcast_tx, _) = tokio::sync::broadcast::channel(1000);

    // Create storage backend
    let storage_path = std::path::PathBuf::from(&config.hub.artifact_storage_path);
    tokio::fs::create_dir_all(&storage_path).await
        .map_err(|e| anyhow::anyhow!("Failed to create storage directory: {}", e))?;

    let storage = Arc::new(crate::storage::LocalStorage::new(storage_path));

    // Create embedding channel (immediate trigger on publish) and cancellation token
    let (embedding_tx, embedding_rx) = tokio::sync::mpsc::channel::<String>(1000);
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Build HybridEncoder (semantic + structured)
    let semantic_encoder = embedding::semantic::SemanticEncoder::new()?;
    let structured_encoder = embedding::structured::StructuredEncoder::new(
        Arc::new(crate::domain::condition::ConditionRegistry::new()),
        config.hub.structured_vector_reserved_dims,
        2,  // dims_per_numeric_key
        4,  // dims_per_categorical_key
    )?;
    let hybrid_encoder = embedding::hybrid::HybridEncoder::new(semantic_encoder, structured_encoder)?;
    let hybrid_dim = config.total_embedding_dimension();
    tracing::info!("HybridEncoder ready — {} dimensions (semantic {} + structured {})",
        hybrid_dim, config.hub.embedding_dimension, config.hub.structured_vector_reserved_dims);

    let state = state::AppState::new(pool, config.clone(), sse_broadcast_tx, storage, embedding_tx)
        .await?;

    tracing::info!("rspc-style endpoint configured at /api/rspc");

    // Start background workers
    let embedding_worker = workers::embedding_queue::EmbeddingQueue::new(
        state.pool.clone(),
        (*state.config).clone(),
        state.graph_cache.clone(),
        state.condition_registry.clone(),
        hybrid_encoder,
        embedding_rx,
        cancel_token.clone(),
    );
    
    let claims_worker = workers::claims::ClaimsExpiryWorker::new(state.pool.clone());
    let bounty_worker = workers::bounty::BountyWorker::new(
        state.pool.clone(),
        state.config.workers.bounty_needed_novelty_threshold,
        state.config.pheromone.exploration_samples,
        state.config.pheromone.exploration_density_radius,
        state.config.hub.embedding_dimension,
        state.config.workers.bounty_sparse_region_max_atoms,
        state.config.hub.neighbourhood_radius,
    );
    let decay_worker = workers::decay::DecayWorker::new(
        state.pool.clone(),
        (*state.config).clone(),
    );
    let lifecycle_worker = workers::lifecycle::LifecycleWorker::new(
        state.pool.clone(),
        crate::domain::lifecycle::LifecycleEvaluator::new(
            state.config.pheromone.disagreement_threshold,
        ),
        cancel_token.clone(),
        state.sse_broadcast_tx.clone(),
        state.config.workers.lifecycle_check_interval_minutes,
    );

    // Spawn workers — all receive a clone of cancel_token for cooperative shutdown.
    let bounty_interval = state.config.workers.staleness_check_interval_minutes;

    let embedding_handle = tokio::spawn({
        async move { embedding_worker.start().await }
    });
    let claims_handle = tokio::spawn({
        let ct = cancel_token.clone();
        async move { claims_worker.start(ct).await }
    });
    let bounty_handle = tokio::spawn({
        let ct = cancel_token.clone();
        async move { bounty_worker.start(bounty_interval, ct).await }
    });
    let decay_handle = tokio::spawn({
        let ct = cancel_token.clone();
        async move { decay_worker.start(ct).await }
    });
    let lifecycle_handle = tokio::spawn(lifecycle_worker.start());

    let metrics_collector = metrics::collector::MetricsCollector::new(
        state.pool.clone(),
        state.config.workers.metrics_collection_interval_seconds,
        cancel_token.clone(),
        state.config.workers.frontier_diversity_k,
    );
    let metrics_handle = tokio::spawn(async move { metrics_collector.start().await });

    // Spawn MCP session cleanup (every 5 minutes)
    let session_store_sweep = state.session_store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            session_store_sweep.cleanup_expired_sessions();
            tracing::debug!("MCP session sweep complete");
        }
    });

    // Build router
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let shared_state = std::sync::Arc::new(state.clone());

    // Routes protected by owner JWT
    let protected_routes = Router::new()
        .route("/review", get(api::handlers::get_review_queue))
        .route("/review/:id", post(api::handlers::review_atom))
        .route("/admin/trigger-bounty-tick", post(api::handlers::trigger_bounty_tick))
        .route("/admin/export", get(api::handlers::export_data))
        // Project write endpoints
        .route("/projects", post(api::projects::create_project))
        .route("/projects/:project_id", axum::routing::delete(api::projects::delete_project))
        .route("/projects/:project_id/protocol", put(api::projects::set_protocol))
        .route("/projects/:project_id/requirements", put(api::projects::set_requirements))
        .route("/projects/:project_id/seed-bounty", put(api::projects::set_seed_bounty))
        .route("/projects/:project_id/files/:filename",
            put(api::projects::upload_file)
                .delete(api::projects::delete_file))
        .layer(middleware::from_fn_with_state(
            shared_state.clone(),
            api::auth::owner_jwt_middleware,
        ));

    let app = Router::new()
        // Public endpoints (no auth required)
        .route("/health", get(api::handlers::health_check))
        .route("/metrics", get(api::handlers::metrics))
        .route("/register", post(api::handlers::register_agent))
        .route("/admin/login", post(api::handlers::admin_login))
        .route("/events", get(api::sse::sse_events))
        // Project read endpoints (public — agents fetch at launch)
        .route("/projects", get(api::projects::list_projects))
        .route("/projects/:project_id", get(api::projects::get_project))
        .route("/projects/:project_id/protocol", get(api::projects::get_protocol))
        .route("/projects/:project_id/requirements", get(api::projects::get_requirements))
        .route("/projects/:project_id/seed-bounty", get(api::projects::get_seed_bounty))
        .route("/projects/:project_id/files", get(api::projects::list_files))
        .route("/projects/:project_id/files/:filename", get(api::projects::get_file))
        // Agent-authenticated endpoints
        .route("/rpc", post(api::rpc::handle_mcp))
        .route("/mcp", post(api::mcp_server::handle_mcp_request)
            .get(api::mcp_server::handle_mcp_get)
            .delete(api::mcp_server::handle_mcp_delete))
        .route("/api/rspc", post(api::rspc_router::handle_rspc_request))
        // Artifact routes
        .route("/artifacts/:hash", put(api::artifacts::put_artifact))
        .route("/artifacts/:hash", get(api::artifacts::get_artifact))
        .route("/artifacts/:hash", head(api::artifacts::head_artifact))
        .route("/artifacts/:hash/meta", get(api::artifacts::get_artifact_metadata))
        .route("/artifacts/:hash/ls", get(api::artifacts::list_artifact_tree))
        .route("/artifacts/:hash/resolve/*path", get(api::artifacts::resolve_artifact_path))
        // JWT-protected routes
        .merge(protected_routes)
        // Global IP rate limiter (skip for authenticated agents)
        .layer(middleware::from_fn_with_state(
            shared_state.clone(),
            api::auth::ip_rate_limit_middleware,
        ))
        .layer(cors)
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB limit
        .with_state(shared_state);

    // Start server (with_connect_info enables IP extraction in middleware)
    let listener = tokio::net::TcpListener::bind(&config.hub.listen_address).await?;
    tracing::info!("Asenix server listening on {}", config.hub.listen_address);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
        .with_graceful_shutdown(shutdown_signal(
            cancel_token,
            embedding_handle,
            claims_handle,
            bounty_handle,
            decay_handle,
            lifecycle_handle,
            metrics_handle,
        ))
        .await?;

    tracing::info!("Clean shutdown completed");
    Ok(())
}

async fn shutdown_signal(
    cancel_token: tokio_util::sync::CancellationToken,
    embedding_handle: tokio::task::JoinHandle<()>,
    claims_handle: tokio::task::JoinHandle<()>,
    bounty_handle: tokio::task::JoinHandle<()>,
    decay_handle: tokio::task::JoinHandle<()>,
    lifecycle_handle: tokio::task::JoinHandle<()>,
    metrics_handle: tokio::task::JoinHandle<()>,
) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).unwrap();
        let mut quit = signal(SignalKind::quit()).unwrap();
        let mut interrupt = signal(SignalKind::interrupt()).unwrap();

        tokio::select! {
            _ = terminate.recv() => {},
            _ = quit.recv() => {},
            _ = interrupt.recv() => {},
        }
    }
    #[cfg(windows)]
    {
        use tokio::signal::windows;
        let mut shutdown = windows::shutdown().unwrap();
        let _ = shutdown.recv().await;
    }

    tracing::info!("Shutdown signal received, starting graceful shutdown");
    cancel_token.cancel();

    tracing::info!("Waiting for all workers to finish (timeout 10s)...");
    tokio::time::timeout(Duration::from_secs(10), async {
        let _ = tokio::join!(
            embedding_handle,
            claims_handle,
            bounty_handle,
            decay_handle,
            lifecycle_handle,
            metrics_handle,
        );
    })
    .await
    .ok();

    tracing::info!("Graceful shutdown sequence completed");
}

