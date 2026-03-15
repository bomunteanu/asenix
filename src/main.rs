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
    let (embedding_queue_tx, _embedding_queue_rx) = tokio::sync::mpsc::channel(1000);
    let (sse_broadcast_tx, _) = tokio::sync::broadcast::channel(1000);
    
    // Create storage backend
    let storage_path = std::path::PathBuf::from(&config.hub.artifact_storage_path);
    tokio::fs::create_dir_all(&storage_path).await
        .map_err(|e| anyhow::anyhow!("Failed to create storage directory: {}", e))?;
    
    let storage = Arc::new(crate::storage::LocalStorage::new(storage_path));
    
    // Initialize embedding provider (local ONNX or OpenAI API)
    let embedding_provider = embedding::provider::EmbeddingProvider::from_env()?;
    let provider_dim = embedding_provider.dimension();
    let config_dim = config.hub.embedding_dimension;
    if provider_dim != config_dim {
        anyhow::bail!(
            "Embedding dimension mismatch: provider '{}' produces {} dims but config.toml \
             has embedding_dimension = {}. Update config.toml to match.",
            embedding_provider.name(), provider_dim, config_dim
        );
    }
    tracing::info!(
        "Embedding provider '{}' ready — {} dimensions",
        embedding_provider.name(), provider_dim
    );
    let embedding_provider = Arc::new(embedding_provider);

    let state = state::AppState::new(pool, config.clone(), embedding_queue_tx.clone(), sse_broadcast_tx, storage)
        .await?;

    tracing::info!("rspc-style endpoint configured at /api/rspc");

    // Start background workers
    let embedding_worker = workers::embedding_queue::EmbeddingQueue::new(
        state.pool.clone(),
        (*state.config).clone(),
        state.graph_cache.clone(),
        state.condition_registry.clone(),
        embedding_provider,
    );
    
    let claims_worker = workers::claims::ClaimsExpiryWorker::new(state.pool.clone());
    let staleness_worker = workers::staleness::StalenessWorker::new(
        state.pool.clone(),
        state.config.hub.neighbourhood_radius,
        state.config.workers.bounty_needed_novelty_threshold,
        state.sse_broadcast_tx.clone(),
    );
    let bounty_worker = workers::bounty::BountyWorker::new(
        state.pool.clone(),
        state.config.workers.bounty_needed_novelty_threshold,
        state.config.pheromone.exploration_samples,
        state.config.pheromone.exploration_density_radius,
        state.config.hub.embedding_dimension,
        state.config.workers.bounty_sparse_region_max_atoms,
    );
    let decay_worker = workers::decay::DecayWorker::new(
        state.pool.clone(),
        (*state.config).clone(),
    );

    // Spawn workers
    let embedding_handle = tokio::spawn(async move {
        embedding_worker.start().await;
    });
    let _claims_handle = tokio::spawn(claims_worker.start());
    let staleness_interval = state.config.workers.staleness_check_interval_minutes;
    let _staleness_handle = tokio::spawn(staleness_worker.start(staleness_interval));
    let _bounty_handle = tokio::spawn(bounty_worker.start(staleness_interval));
    let decay_interval = state.config.workers.decay_interval_minutes;
    let _decay_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(decay_interval * 60));
        loop {
            interval.tick().await;
            match decay_worker.run_decay_sweep().await {
                Ok(n) if n > 0 => tracing::info!("Decay sweep updated {} atoms", n),
                Ok(_) => {}
                Err(e) => tracing::error!("Decay sweep failed: {}", e),
            }
        }
    });

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
            embedding_queue_tx,
            embedding_handle,
        ))
        .await?;

    tracing::info!("Clean shutdown completed");
    Ok(())
}

async fn shutdown_signal(
    embedding_queue_tx: tokio::sync::mpsc::Sender<String>,
    mut embedding_handle: tokio::task::JoinHandle<()>,
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

    // Step 1: Stop accepting new connections (handled by axum's graceful shutdown)
    
    // Step 2: Close embedding queue sender to stop processing new atoms
    drop(embedding_queue_tx);
    tracing::info!("Closed embedding queue sender");

    // Step 3: Wait for embedding worker to finish with timeout
    tracing::info!("Waiting for embedding worker to finish...");
    match tokio::time::timeout(Duration::from_secs(30), &mut embedding_handle).await {
        Ok(Ok(())) => tracing::info!("Embedding worker finished gracefully"),
        Ok(Err(e)) => tracing::error!("Embedding worker panicked: {}", e),
        Err(_) => {
            tracing::warn!("Embedding worker did not finish within timeout");
            // Force cancel
            embedding_handle.abort();
        }
    }

    // Step 4: Cancel background workers (they will be cleaned up automatically)
    tracing::info!("Background workers will be cleaned up automatically");

    // Step 5: Close database pool (handled by Drop implementation)
    tracing::info!("Graceful shutdown sequence completed");
}

