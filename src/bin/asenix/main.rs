mod client;
mod config;
mod display;
mod domain;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::io::IsTerminal;

// ─── CLI Definition ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "asenix",
    about = "CLI for the Asenix AI research coordination hub",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Asenix stack via docker compose and wait for readiness
    Up,

    /// Stop the Asenix stack via docker compose
    Down,

    /// Show hub health and statistics
    Status {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
    },

    /// Manage research agents
    Agent {
        #[command(subcommand)]
        subcommand: AgentCommands,
    },

    /// Manage domain packs (agent instructions + seed bounties)
    Domain {
        #[command(subcommand)]
        subcommand: DomainCommands,
    },

    /// Manage research bounties
    Bounty {
        #[command(subcommand)]
        subcommand: BountyCommands,
    },

    /// Review pending atoms (requires `asenix login` first)
    Queue {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
    },

    /// Authenticate as hub owner and save a JWT token
    Login {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
    },

    /// View agent logs (tail one or multiplex all)
    Logs {
        /// Agent number to tail; omit to multiplex all agents
        n: Option<usize>,
    },

    /// Delete local credentials and logs for this host
    Reset {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
    },

    /// Manage projects on the hub
    Project {
        #[command(subcommand)]
        subcommand: ProjectCommands,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// Register agents and launch them via claude CLI (reads protocol/files from hub)
    Run {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        /// Project slug on the hub (e.g. cifar10-resnet)
        #[arg(long)]
        project: String,
        /// Number of agents to launch in parallel
        #[arg(long, short, default_value = "1")]
        n: usize,
    },

    /// Stop background agents launched with `agent run -n N`
    Stop {
        /// Project slug to stop; omit to stop all running agents
        #[arg(long)]
        project: Option<String>,
        /// Stop only agent number N (default: all agents for the project)
        #[arg(long, short)]
        n: Option<usize>,
    },

    /// List all registered agents on this machine
    List,
}

#[derive(Subcommand)]
enum ProjectCommands {
    /// Create a new project on the hub
    Create {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        slug: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// List all projects on the hub
    List {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
    },
    /// Show details for a project
    Show {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        /// Project slug
        slug: String,
    },
    /// Delete a project
    Delete {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        /// Project slug
        slug: String,
    },
    /// Manage the project protocol (CLAUDE.md)
    Protocol {
        #[command(subcommand)]
        subcommand: ProtocolCommands,
    },
    /// Manage project files
    Files {
        #[command(subcommand)]
        subcommand: FilesCommands,
    },
    /// Manage Python requirements for the project
    Requirements {
        #[command(subcommand)]
        subcommand: RequirementsCommands,
    },
    /// Manage the project seed bounty
    SeedBounty {
        #[command(subcommand)]
        subcommand: SeedBountyCommands,
    },
}

#[derive(Subcommand)]
enum ProtocolCommands {
    /// Set the protocol (reads from --file or opens $EDITOR)
    Set {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
        /// Path to a file to use as protocol; omit to open $EDITOR
        #[arg(long)]
        file: Option<String>,
    },
    /// Print the current protocol
    Show {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
    },
}

#[derive(Subcommand)]
enum FilesCommands {
    /// Upload a file to the project
    Upload {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
        /// Local file path to upload
        filepath: String,
        /// Override the filename stored on the hub
        #[arg(long)]
        name: Option<String>,
    },
    /// List files attached to the project
    List {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
    },
    /// Download a file from the project
    Download {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
        filename: String,
        /// Where to write the file; defaults to ./<filename>
        #[arg(long)]
        out: Option<String>,
    },
    /// Delete a file from the project
    Delete {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
        filename: String,
    },
}

#[derive(Subcommand)]
enum RequirementsCommands {
    /// Set requirements (reads from --file or opens $EDITOR)
    Set {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
        /// Path to a requirements.txt file; omit to open $EDITOR
        #[arg(long)]
        file: Option<String>,
    },
    /// Print the current requirements
    Show {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
    },
}

#[derive(Subcommand)]
enum SeedBountyCommands {
    /// Set the seed bounty (reads from --file or opens $EDITOR)
    Set {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
        /// Path to a JSON file; omit to open $EDITOR
        #[arg(long)]
        file: Option<String>,
    },
    /// Print the current seed bounty
    Show {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        slug: String,
    },
}

#[derive(Subcommand)]
enum DomainCommands {
    /// Install a domain pack from a local directory
    Install {
        /// Path to the domain pack directory (must contain domain.toml)
        path: String,
    },

    /// List all installed domain packs
    List,
}

#[derive(Subcommand)]
enum BountyCommands {
    /// Interactively post a new research bounty
    Post {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        #[arg(long, default_value = "general")]
        domain: String,
    },

    /// List open bounties
    List {
        #[arg(long, env = "ASENIX_HUB", default_value = "http://localhost:3000")]
        hub: String,
        #[arg(long)]
        domain: Option<String>,
    },
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

fn main() {
    if let Err(e) = run() {
        display::error(&e.to_string());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Up => cmd_up(),
        Commands::Down => cmd_down(),
        Commands::Status { hub } => cmd_status(&hub),
        Commands::Agent { subcommand } => match subcommand {
            AgentCommands::Run { hub, project, n } => cmd_agent_run(&hub, &project, n),
            AgentCommands::Stop { project, n } => cmd_agent_stop(project.as_deref(), n),
            AgentCommands::List => cmd_agent_list(),
        },
        Commands::Domain { subcommand } => match subcommand {
            DomainCommands::Install { path } => cmd_domain_install(&path),
            DomainCommands::List => cmd_domain_list(),
        },
        Commands::Bounty { subcommand } => match subcommand {
            BountyCommands::Post { hub, domain } => cmd_bounty_post(&hub, &domain),
            BountyCommands::List { hub, domain } => {
                cmd_bounty_list(&hub, domain.as_deref())
            }
        },
        Commands::Queue { hub } => cmd_queue(&hub),
        Commands::Login { hub } => cmd_login(&hub),
        Commands::Logs { n } => cmd_logs(n),
        Commands::Reset { hub } => cmd_reset(&hub),
        Commands::Project { subcommand } => match subcommand {
            ProjectCommands::Create { hub, name, slug, description } => {
                cmd_project_create(&hub, &name, &slug, description.as_deref())
            }
            ProjectCommands::List { hub } => cmd_project_list(&hub),
            ProjectCommands::Show { hub, slug } => cmd_project_show(&hub, &slug),
            ProjectCommands::Delete { hub, slug } => cmd_project_delete(&hub, &slug),
            ProjectCommands::Protocol { subcommand } => match subcommand {
                ProtocolCommands::Set { hub, slug, file } => {
                    cmd_project_protocol_set(&hub, &slug, file.as_deref())
                }
                ProtocolCommands::Show { hub, slug } => cmd_project_protocol_show(&hub, &slug),
            },
            ProjectCommands::Files { subcommand } => match subcommand {
                FilesCommands::Upload { hub, slug, filepath, name } => {
                    cmd_project_files_upload(&hub, &slug, &filepath, name.as_deref())
                }
                FilesCommands::List { hub, slug } => cmd_project_files_list(&hub, &slug),
                FilesCommands::Download { hub, slug, filename, out } => {
                    cmd_project_files_download(&hub, &slug, &filename, out.as_deref())
                }
                FilesCommands::Delete { hub, slug, filename } => {
                    cmd_project_files_delete(&hub, &slug, &filename)
                }
            },
            ProjectCommands::Requirements { subcommand } => match subcommand {
                RequirementsCommands::Set { hub, slug, file } => {
                    cmd_project_requirements_set(&hub, &slug, file.as_deref())
                }
                RequirementsCommands::Show { hub, slug } => {
                    cmd_project_requirements_show(&hub, &slug)
                }
            },
            ProjectCommands::SeedBounty { subcommand } => match subcommand {
                SeedBountyCommands::Set { hub, slug, file } => {
                    cmd_project_seed_bounty_set(&hub, &slug, file.as_deref())
                }
                SeedBountyCommands::Show { hub, slug } => {
                    cmd_project_seed_bounty_show(&hub, &slug)
                }
            },
        },
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Walk parent directories until docker-compose.yml is found.
fn find_project_root() -> Result<std::path::PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join("docker-compose.yml").exists() || dir.join("docker-compose.yaml").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            anyhow::bail!(
                "docker-compose.yml not found in current directory or any parent\n  \
                 hint: Run from the Asenix project root"
            );
        }
    }
}

// ─── Commands ────────────────────────────────────────────────────────────────

fn cmd_up() -> Result<()> {
    let project_dir = find_project_root()?;
    display::progress(&format!(
        "Starting Asenix stack in {}...",
        project_dir.display()
    ));

    let status = std::process::Command::new("docker")
        .args(["compose", "up", "-d"])
        .current_dir(&project_dir)
        .status()
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to run docker compose: {}\n  hint: Is Docker installed and running?",
                e
            )
        })?;

    if !status.success() {
        display::error("docker compose up failed");
        display::hint("Check `docker compose logs` for details");
        std::process::exit(1);
    }

    display::progress("Waiting for hub to be ready...");
    let hub = "http://localhost:3000";
    let client = client::AsenixClient::new(hub);
    let spinner = display::Spinner::new("Checking health...");
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(30);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Ok(h) = client.health() {
            if h.status == "healthy" {
                spinner.finish_success(&format!(
                    "Hub ready at {}  atoms: {}  queue: {}",
                    hub, h.graph_nodes, h.embedding_queue_depth
                ));
                return Ok(());
            }
        }
        if start.elapsed() > timeout {
            spinner.finish_error("Hub did not become ready within 30s");
            display::hint("Check logs with `docker compose logs asenix`");
            std::process::exit(1);
        }
        spinner.set_message(format!(
            "Waiting... ({:.0}s elapsed)",
            start.elapsed().as_secs_f32()
        ));
    }
}

fn cmd_down() -> Result<()> {
    let project_dir = find_project_root()?;
    display::progress("Stopping Asenix stack...");

    let status = std::process::Command::new("docker")
        .args(["compose", "down"])
        .current_dir(&project_dir)
        .status()
        .map_err(|e| {
            anyhow::anyhow!(
                "failed to run docker compose: {}\n  hint: Is Docker installed and running?",
                e
            )
        })?;

    if !status.success() {
        display::error("docker compose down failed");
        display::hint("Check `docker compose logs` for details");
        std::process::exit(1);
    }
    display::success("Stack stopped");
    Ok(())
}

fn cmd_status(hub: &str) -> Result<()> {
    let client = client::AsenixClient::new(hub);
    let spinner = display::Spinner::new("Fetching status...");

    match client.health() {
        Ok(h) => {
            spinner.finish_success(&format!("Hub: {}", hub));
            display::print_table(
                &["Field", "Value"],
                &[
                    vec!["Status".to_string(), h.status],
                    vec!["Database".to_string(), h.database],
                    vec!["Graph nodes".to_string(), h.graph_nodes.to_string()],
                    vec!["Graph edges".to_string(), h.graph_edges.to_string()],
                    vec!["Embed queue".to_string(), h.embedding_queue_depth.to_string()],
                ],
            );
        }
        Err(_) => {
            spinner.finish_error(&format!("Cannot reach hub at {}", hub));
            display::hint("Run `asenix up` to start the server, or pass --hub with the correct URL");
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_agent_run(hub: &str, project_slug: &str, n: usize) -> Result<()> {
    use std::io::{self, BufRead, BufReader, Write};
    use std::process::Stdio;

    // ── STEP 1: Pre-flight ────────────────────────────────────────────────────
    display::progress(&format!(
        "Launching {} agent(s) for project '{}'",
        n, project_slug
    ));

    let api_client = client::AsenixClient::new(hub);

    match api_client.health() {
        Ok(_) => display::success(&format!("Hub reachable ({})", hub)),
        Err(_) => {
            display::error(&format!("Cannot reach hub at {}", hub));
            display::hint("Run `asenix up` first");
            std::process::exit(1);
        }
    }

    // Resolve project from hub
    let project = match api_client.get_project_by_slug(project_slug) {
        Ok(Some(p)) => {
            display::success(&format!("Project '{}' found (id: {})", project_slug, display::truncate(&p.project_id, 14)));
            p
        }
        Ok(None) => {
            display::error(&format!("Project '{}' not found on hub", project_slug));
            display::hint("Run `asenix project list` to see available projects");
            std::process::exit(1);
        }
        Err(e) => {
            display::error(&format!("Failed to fetch project: {}", e));
            std::process::exit(1);
        }
    };

    if !domain::claude_in_path() {
        display::error("claude not found in PATH");
        display::hint("Install with: npm install -g @anthropic-ai/claude-code");
        std::process::exit(1);
    }
    display::success("claude found");

    // Fetch protocol (CLAUDE.md) from hub
    let claude_md = match api_client.get_protocol(&project.project_id) {
        Ok(Some(text)) => {
            display::success("Protocol (CLAUDE.md) loaded from hub");
            text
        }
        Ok(None) => {
            println!("  {} No protocol set for this project — agents will run without instructions", "⚠".yellow());
            String::new()
        }
        Err(e) => {
            display::error(&format!("Failed to fetch protocol: {}", e));
            std::process::exit(1);
        }
    };

    // Fetch requirements from hub
    let requirements: Vec<String> = match api_client.get_requirements(&project.project_id) {
        Ok(reqs) => {
            let pkgs: Vec<String> = reqs
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            if !pkgs.is_empty() {
                display::success(&format!("{} Python requirement(s) loaded from hub", pkgs.len()));
            }
            pkgs
        }
        Err(_) => vec![],
    };

    // Fetch project files list
    let project_files = match api_client.list_project_files(&project.project_id) {
        Ok(files) => {
            if !files.is_empty() {
                display::success(&format!("{} project file(s) found on hub", files.len()));
            }
            files
        }
        Err(_) => vec![],
    };

    // ── STEP 2: Python deps ───────────────────────────────────────────────────
    if !requirements.is_empty() {
        display::progress("Installing Python dependencies...");
        // Write a temp requirements.txt and pip install it
        let tmp_reqs = std::env::temp_dir().join(format!("asenix_{}_reqs.txt", project_slug));
        std::fs::write(&tmp_reqs, requirements.join("\n"))?;
        let output = std::process::Command::new("pip")
            .args(["install", "-r", tmp_reqs.to_str().unwrap_or(""), "--quiet"])
            .output();
        match output {
            Ok(o) if o.status.success() => display::success("Dependencies installed"),
            Ok(o) => {
                display::error("pip install failed");
                let stderr = String::from_utf8_lossy(&o.stderr);
                if !stderr.trim().is_empty() {
                    eprintln!("{}", stderr.trim());
                }
                display::hint("Fix your Python environment and retry");
                std::process::exit(1);
            }
            Err(e) => {
                display::error(&format!("Failed to run pip: {}", e));
                display::hint("Make sure pip is installed and in PATH");
                std::process::exit(1);
            }
        }
    }

    // ── STEP 3: Register first agent + seed bounty ────────────────────────────
    let hostname = config::hostname();
    std::fs::create_dir_all(config::logs_dir())?;

    let first_n = config::next_agent_n_for_domain(&hostname, project_slug);
    let first_cred = register_one_agent(&api_client, hub, project_slug, first_n, &hostname)?;
    display::success(&format!(
        "Agent {} registered (agent_id: {})",
        first_n,
        display::truncate(&first_cred.agent_id, 20)
    ));

    let atom_count = get_project_atom_count(&api_client, &project.project_id, &first_cred);
    if atom_count == 0 {
        match api_client.get_seed_bounty(&project.project_id) {
            Ok(Some(bounty)) => {
                display::progress("No atoms found — posting seed bounty from hub...");
                // The stored bounty may be wrapped as {"atoms": [...]} or be a bare atom object.
                // Normalise to a flat Vec of atom objects, injecting project_id into each.
                let raw_atoms: Vec<serde_json::Value> =
                    if let Some(arr) = bounty.get("atoms").and_then(|v| v.as_array()) {
                        arr.clone()
                    } else {
                        vec![bounty.clone()]
                    };
                let atoms: Vec<serde_json::Value> = raw_atoms
                    .into_iter()
                    .map(|mut a| {
                        if let Some(obj) = a.as_object_mut() {
                            if !obj.contains_key("atom_type") {
                                obj.insert("atom_type".into(), "bounty".into());
                            }
                            obj.insert("project_id".into(), project.project_id.as_str().into());
                        }
                        a
                    })
                    .collect();
                // Publish each seed atom via the `publish` tool (one atom per call).
                let mut last_atom_id = String::from("unknown");
                let mut publish_ok = true;
                for atom in &atoms {
                    let mut payload = atom.clone();
                    if let Some(obj) = payload.as_object_mut() {
                        obj.insert("agent_id".into(), first_cred.agent_id.as_str().into());
                        obj.insert("api_token".into(), first_cred.api_token.as_str().into());
                        obj.entry("project_id").or_insert_with(|| project.project_id.as_str().into());
                        obj.entry("provenance").or_insert(serde_json::json!({}));
                    }
                    match api_client.mcp_call("publish", payload) {
                        Ok(result) => {
                            if let Some(id) = result.get("atom_id").and_then(|v| v.as_str()) {
                                last_atom_id = id.to_string();
                            }
                        }
                        Err(e) => {
                            publish_ok = false;
                            println!("  {} Seed atom post failed ({})", "⚠".yellow(), e);
                            break;
                        }
                    }
                }
                if publish_ok {
                    display::success(&format!("Seed bounty posted (atom_id: {})", last_atom_id));
                } else {
                    print!("  Agents may stall with nothing to explore. Continue? [y/N]: ");
                    io::stdout().flush()?;
                    let mut ans = String::new();
                    io::stdin().read_line(&mut ans)?;
                    if ans.trim().to_lowercase() != "y" {
                        println!("Cancelled.");
                        std::process::exit(1);
                    }
                }
            }
            Ok(None) => {
                println!(
                    "  {} No seed bounty on hub and no atoms yet — agents may have nothing to explore",
                    "⚠".yellow()
                );
            }
            Err(_) => {}
        }
    } else {
        display::success(&format!(
            "Project has {} existing atom(s) — skipping seed bounty",
            atom_count
        ));
    }

    // ── STEP 4: Register remaining agents ────────────────────────────────────
    let mut agents: Vec<(usize, config::AgentCred)> = vec![(first_n, first_cred)];
    for _ in 1..n {
        let agent_n = config::next_agent_n_for_domain(&hostname, project_slug);
        let cred = register_one_agent(&api_client, hub, project_slug, agent_n, &hostname)?;
        display::success(&format!(
            "Agent {} registered (agent_id: {})",
            agent_n,
            display::truncate(&cred.agent_id, 20)
        ));
        agents.push((agent_n, cred));
    }

    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let mcp_url = format!("{}/mcp", hub);
    let mcp_config_json = serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "asenix": { "type": "http", "url": mcp_url }
        }
    }))?;

    // ── STEP 5: Setup workdirs and launch ─────────────────────────────────────
    display::progress("Preparing working directories...");
    let mut children: Vec<(usize, std::process::Child, std::path::PathBuf)> = Vec::new();

    for (agent_n, cred) in &agents {
        let workdir = config::workdir_path(project_slug, *agent_n);
        std::fs::create_dir_all(&workdir)?;

        // Download project files from hub into workdir
        for file_info in &project_files {
            match api_client.download_project_file(&project.project_id, &file_info.filename) {
                Ok(bytes) => {
                    std::fs::write(workdir.join(&file_info.filename), &bytes)?;
                }
                Err(e) => {
                    println!(
                        "  {} Failed to download '{}': {}",
                        "⚠".yellow(),
                        file_info.filename,
                        e
                    );
                }
            }
        }

        // Write .agent_config
        std::fs::write(
            workdir.join(".agent_config"),
            format!(
                "AGENT_ID={}\nAPI_TOKEN={}\nSERVER_URL={}\nAGENT_NAME={}-{}-{}\nPROJECT_ID={}\n",
                cred.agent_id, cred.api_token, hub, hostname, project_slug, agent_n,
                project.project_id
            ),
        )?;

        // Write MCP config
        std::fs::write(workdir.join("mcp_config.json"), &mcp_config_json)?;

        // Build prompt
        let prompt = if claude_md.is_empty() {
            format!(
                "Your credentials: AGENT_ID={agent_id} API_TOKEN={api_token} PROJECT_ID={project_id}\n\
                 Working directory: {workdir}\n\
                 The MCP server \"asenix\" is configured — use it for all hub calls.\n\
                 Project: {project_name} (id: {project_id})\n\
                 IMPORTANT: Every atom you publish must include \"project_id\": \"{project_id}\".\n\
                 IMPORTANT: Every survey call must include \"project_id\": \"{project_id}\".",
                agent_id = cred.agent_id,
                api_token = cred.api_token,
                workdir = workdir.display(),
                project_name = project.name,
                project_id = project.project_id,
            )
        } else {
            format!(
                "Your credentials: AGENT_ID={agent_id} API_TOKEN={api_token} PROJECT_ID={project_id}\n\
                 Working directory: {workdir}\n\
                 The MCP server \"asenix\" is configured — use it for all hub calls.\n\
                 IMPORTANT: Every atom you publish must include \"project_id\": \"{project_id}\" — this is mandatory.\n\
                 IMPORTANT: Every survey call must include \"project_id\": \"{project_id}\" — this scopes results to this project.\n\
                 Follow all instructions in the document below exactly.\n\n\
                 {claude_md}",
                agent_id = cred.agent_id,
                api_token = cred.api_token,
                project_id = project.project_id,
                workdir = workdir.display(),
                claude_md = claude_md,
            )
        };

        let log_path = config::domain_log_path(project_slug, *agent_n, &ts);
        let mcp_config_path = workdir.join("mcp_config.json");

        if n == 1 {
            display::progress(&format!(
                "Launching agent {} (Ctrl+C to stop)...",
                agent_n
            ));
            display::hint(&format!("Log: {}", log_path.display()));

            let atoms_before = get_project_atom_count(&api_client, &project.project_id, cred);

            let log_file = std::fs::File::create(&log_path)?;

            let mut cmd = std::process::Command::new("claude");
            cmd.args([
                "--dangerously-skip-permissions",
                "--output-format",
                "stream-json",
                "--verbose",
                "--mcp-config",
                mcp_config_path.to_str().unwrap_or(""),
                "-p",
                &prompt,
            ])
            .current_dir(&workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

            // Spawn in its own process group so Ctrl+C kills claude AND all
            // subprocesses it starts (e.g. python train.py).
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }

            let mut child = cmd.spawn().map_err(|e| {
                anyhow::anyhow!(
                    "failed to launch claude: {}\n  hint: npm install -g @anthropic-ai/claude-code",
                    e
                )
            })?;

            let child_pid = child.id();
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();

            let log_arc = std::sync::Arc::new(std::sync::Mutex::new(log_file));
            let log_arc2 = log_arc.clone();

            // Track whether the agent ever called publish_atoms.
            let saw_publish = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let saw_publish_clone = saw_publish.clone();

            // Kill the child's entire process group on Ctrl+C.
            let _ = ctrlc::set_handler(move || {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-(child_pid as libc::pid_t), libc::SIGTERM);
                }
                std::process::exit(130);
            });

            let t_out = std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().flatten() {
                    // Write raw JSON to log.
                    if let Ok(mut f) = log_arc.lock() {
                        let _ = std::io::Write::write_fmt(&mut *f, format_args!("{}\n", line));
                    }
                    // Track publish tool calls.
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                        if v["type"].as_str() == Some("tool_use")
                            && v["name"].as_str().map_or(false, |n| n == "publish" || n.contains("publish"))
                        {
                            saw_publish_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    // Pretty-print the event.
                    print_stream_event(&line);
                }
            });
            let t_err = std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    if let Ok(mut f) = log_arc2.lock() {
                        let _ = std::io::Write::write_fmt(&mut *f, format_args!("{}\n", line));
                    }
                    eprintln!("{}", line);
                }
            });

            let status = child.wait()?;
            let _ = t_out.join();
            let _ = t_err.join();

            // Post-run summary.
            println!();
            let atoms_after = get_project_atom_count(&api_client, &project.project_id, cred);
            let new_atoms = atoms_after.saturating_sub(atoms_before);
            if new_atoms > 0 {
                display::success(&format!("{} new atom(s) published to hub", new_atoms));
            } else if saw_publish.load(std::sync::atomic::Ordering::Relaxed) {
                // publish was called but atoms may still be embedding.
                display::success("publish called — atoms may still be indexing");
            } else {
                println!(
                    "  {} No atoms published — check the log for details",
                    "⚠".yellow()
                );
                display::hint(&format!("Log: {}", log_path.display()));
            }

            if !status.success() {
                display::error(&format!(
                    "Agent exited with code {}",
                    status.code().unwrap_or(-1)
                ));
                std::process::exit(status.code().unwrap_or(1));
            }
        } else {
            let log_file = std::fs::File::create(&log_path)?;
            let mut cmd = std::process::Command::new("claude");
            cmd.args([
                    "--dangerously-skip-permissions",
                    "--mcp-config",
                    mcp_config_path.to_str().unwrap_or(""),
                    "-p",
                    &prompt,
                ])
                .current_dir(&workdir)
                .stdout(log_file.try_clone()?)
                .stderr(log_file);

            // Own process group so SIGTERM to -pgid kills claude + any train.py child.
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }

            match cmd.spawn() {
                Ok(child) => {
                    // Write PID file so `asenix agent stop` can find this process group.
                    let _ = std::fs::write(
                        config::pid_path(project_slug, *agent_n),
                        child.id().to_string(),
                    );
                    display::progress(&format!("Agent {} → log: {}", agent_n, log_path.display()));
                    children.push((*agent_n, child, log_path));
                }
                Err(e) => {
                    display::error(&format!("Failed to launch agent {}: {}", agent_n, e));
                    display::hint("npm install -g @anthropic-ai/claude-code");
                }
            }
        }
    }

    if !children.is_empty() {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let mut summary_rows: Vec<Vec<String>> = Vec::new();
        for (agent_n, mut child, log_path) in children {
            let status_str = match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(-1);
                    display::error(&format!(
                        "Agent {} exited with code {} — check log",
                        agent_n, code
                    ));
                    format!("exited ({})", code)
                }
                _ => "running".to_string(),
            };
            summary_rows.push(vec![
                agent_n.to_string(),
                display::truncate(
                    &agents
                        .iter()
                        .find(|(n, _)| *n == agent_n)
                        .map(|(_, c)| c.agent_id.as_str())
                        .unwrap_or(""),
                    20,
                ),
                status_str,
                log_path.display().to_string(),
            ]);
        }
        println!();
        display::print_table(&["Agent", "Agent ID", "Status", "Log"], &summary_rows);
    }

    Ok(())
}

fn register_one_agent(
    api_client: &client::AsenixClient,
    hub: &str,
    domain: &str,
    n: usize,
    hostname: &str,
) -> Result<config::AgentCred> {
    let name = format!("{}-{}-agent-{}", hostname, domain, n);
    match api_client.register_simple(&name) {
        Ok(reg) => {
            let cred = config::AgentCred {
                agent_id: reg.agent_id,
                api_token: reg.api_token,
                hub: hub.to_string(),
                domain: domain.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            config::save_agent_cred_for_domain(hostname, domain, n, &cred)?;
            Ok(cred)
        }
        Err(e) => {
            display::error(&format!("Failed to register agent {}: {}", n, e));
            display::hint("Check hub is reachable: `asenix status`");
            std::process::exit(1);
        }
    }
}

/// Returns the number of atoms currently in a project (0 on any error).
fn get_project_atom_count(
    api_client: &client::AsenixClient,
    project_id: &str,
    cred: &config::AgentCred,
) -> usize {
    match api_client.rpc_call(
        "search_atoms",
        serde_json::json!({
            "agent_id": cred.agent_id,
            "api_token": cred.api_token,
            "project_id": project_id,
            "limit": 10000,
        }),
    ) {
        Ok(result) => result["atoms"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        Err(_) => 0,
    }
}

/// Parse a single stream-json event line from `claude --output-format stream-json`
/// and print a human-readable representation.
fn print_stream_event(line: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    match v["type"].as_str() {
        Some("assistant") => {
            if let Some(arr) = v["message"]["content"].as_array() {
                for item in arr {
                    if item["type"].as_str() == Some("text") {
                        if let Some(t) = item["text"].as_str() {
                            print!("{}", t);
                            if !t.ends_with('\n') {
                                println!();
                            }
                        }
                    }
                }
            }
        }
        Some("tool_use") => {
            let name = v["name"].as_str().unwrap_or("?");
            let input = &v["input"];
            if name == "Bash" || name == "bash" || name.ends_with("__bash") {
                // Show the command being run
                if let Some(cmd) = input["command"].as_str() {
                    let cmd_short = cmd.lines().next().unwrap_or(cmd);
                    let truncated = if cmd_short.len() > 120 {
                        format!("{}…", &cmd_short[..120])
                    } else {
                        cmd_short.to_string()
                    };
                    println!("  {} $ {}", "→".cyan(), truncated);
                } else {
                    println!("  {} Bash", "→".cyan());
                }
            } else if name.contains("Read") || name.contains("Write") || name.contains("Edit") || name.contains("Glob") || name.contains("Grep") {
                // File operations — show the path
                let path = input["file_path"]
                    .as_str()
                    .or_else(|| input["pattern"].as_str())
                    .or_else(|| input["path"].as_str())
                    .unwrap_or("?");
                let short = name.split("__").last().unwrap_or(name);
                println!("  {} {} {}", "→".cyan(), short, path);
            } else {
                // MCP / other tools — show tool name and key fields
                let short = name.strip_prefix("mcp__asenix__").unwrap_or(name);
                // Print a one-line summary of the most useful input field
                let detail = if let Some(stmt) = input["statement"].as_str() {
                    format!(": \"{}\"", &stmt[..stmt.len().min(80)])
                } else if let Some(q) = input["query"].as_str() {
                    format!(": \"{}\"", &q[..q.len().min(80)])
                } else if let Some(h) = input["hypothesis"].as_str() {
                    format!(": \"{}\"", &h[..h.len().min(80)])
                } else if let Some(d) = input["domain"].as_str() {
                    format!(" [{}]", d)
                } else {
                    String::new()
                };
                println!("  {} {}{}", "→".cyan(), short, detail);
            }
        }
        Some("result") => {
            if let Some(cost) = v["total_cost_usd"].as_f64() {
                println!("  {} cost: ${:.4}", "ℹ".dimmed(), cost);
            }
        }
        _ => {}
    }
}

// ── Project helpers ───────────────────────────────────────────────────────────

/// Load the owner JWT; exits with a hint if not logged in.
fn load_owner_token(hub: &str) -> String {
    match config::load_auth() {
        Ok(auth) => auth.token,
        Err(_) => {
            display::error("Not logged in");
            display::hint(&format!("Run `asenix login --hub {}`", hub));
            std::process::exit(1);
        }
    }
}

/// Resolve a project slug to a ProjectInfo; exits with a hint if not found.
fn resolve_project(
    api_client: &client::AsenixClient,
    hub: &str,
    slug: &str,
) -> client::ProjectInfo {
    match api_client.get_project_by_slug(slug) {
        Ok(Some(p)) => p,
        Ok(None) => {
            display::error(&format!("Project '{}' not found", slug));
            display::hint(&format!(
                "Run `asenix project list --hub {}` to see available projects",
                hub
            ));
            std::process::exit(1);
        }
        Err(e) => {
            display::error(&format!("Failed to fetch project: {}", e));
            std::process::exit(1);
        }
    }
}

/// Open $EDITOR with `initial` content and return the saved result.
fn open_editor(initial: &str) -> Result<String> {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().context("failed to create temp file")?;
    tmp.write_all(initial.as_bytes())?;
    tmp.flush()?;
    let path = tmp.path().to_owned();

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("failed to launch editor '{}' — set $EDITOR", editor))?;

    if !status.success() {
        anyhow::bail!("editor exited with non-zero status");
    }

    std::fs::read_to_string(&path).context("failed to read temp file after editing")
}

fn format_bytes(n: i64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

fn guess_content_type(filename: &str) -> &'static str {
    match filename.rsplit('.').next().unwrap_or("") {
        "json" => "application/json",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "py" => "text/x-python",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
}

// ── cmd_project_* ─────────────────────────────────────────────────────────────

fn cmd_project_create(hub: &str, name: &str, slug: &str, description: Option<&str>) -> Result<()> {
    let token = load_owner_token(hub);
    let api_client = client::AsenixClient::new(hub);
    let spinner = display::Spinner::new("Creating project...");

    match api_client.create_project(&token, name, slug, description) {
        Ok(p) => {
            spinner.finish_success(&format!("Project '{}' created", p.slug));
            if std::io::stdout().is_terminal() {
                display::print_table(
                    &["Field", "Value"],
                    &[
                        vec!["ID".to_string(), p.project_id],
                        vec!["Name".to_string(), p.name],
                        vec!["Slug".to_string(), p.slug],
                        vec!["Description".to_string(), p.description.unwrap_or_default()],
                    ],
                );
            }
        }
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            display::hint("Check that you are logged in with `asenix login`");
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_list(hub: &str) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let spinner = display::Spinner::new("Fetching projects...");

    match api_client.list_projects() {
        Ok(projects) if projects.is_empty() => {
            spinner.finish_success("No projects found");
            display::hint(&format!(
                "Create one with `asenix project create --hub {} --name ... --slug ...`",
                hub
            ));
        }
        Ok(projects) => {
            spinner.finish_success(&format!("{} project(s)", projects.len()));
            if std::io::stdout().is_terminal() {
                display::print_table(
                    &["Slug", "Name", "Description", "Created"],
                    &projects
                        .iter()
                        .map(|p| {
                            vec![
                                p.slug.clone(),
                                p.name.clone(),
                                display::truncate(p.description.as_deref().unwrap_or(""), 40),
                                p.created_at.chars().take(10).collect(),
                            ]
                        })
                        .collect::<Vec<_>>(),
                );
            } else {
                for p in &projects {
                    println!("{}", p.slug);
                }
            }
        }
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_show(hub: &str, slug: &str) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    if std::io::stdout().is_terminal() {
        display::print_table(
            &["Field", "Value"],
            &[
                vec!["ID".to_string(), p.project_id],
                vec!["Name".to_string(), p.name],
                vec!["Slug".to_string(), p.slug],
                vec!["Description".to_string(), p.description.unwrap_or_default()],
                vec!["Created".to_string(), p.created_at],
            ],
        );
    } else {
        println!("id: {}", p.project_id);
        println!("name: {}", p.name);
        println!("slug: {}", p.slug);
        println!("description: {}", p.description.unwrap_or_default());
        println!("created_at: {}", p.created_at);
    }
    Ok(())
}

fn cmd_project_delete(hub: &str, slug: &str) -> Result<()> {
    use std::io::{self, Write};

    let token = load_owner_token(hub);
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    if std::io::stdin().is_terminal() {
        print!("Delete project '{}' (id: {})? [y/N]: ", p.slug, display::truncate(&p.project_id, 14));
        io::stdout().flush()?;
        let mut ans = String::new();
        io::stdin().read_line(&mut ans)?;
        if ans.trim().to_lowercase() != "y" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let spinner = display::Spinner::new("Deleting...");
    match api_client.delete_project_rest(&token, &p.project_id) {
        Ok(_) => spinner.finish_success(&format!("Project '{}' deleted", slug)),
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_protocol_set(hub: &str, slug: &str, file: Option<&str>) -> Result<()> {
    let token = load_owner_token(hub);
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let text = if let Some(path) = file {
        std::fs::read_to_string(path)
            .with_context(|| format!("failed to read '{}'", path))?
    } else {
        // Fetch current protocol as initial content for editor
        let current = api_client.get_protocol(&p.project_id).unwrap_or(None).unwrap_or_default();
        open_editor(&current)?
    };

    let spinner = display::Spinner::new("Saving protocol...");
    match api_client.set_protocol(&token, &p.project_id, &text) {
        Ok(_) => spinner.finish_success("Protocol saved"),
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_protocol_show(hub: &str, slug: &str) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    match api_client.get_protocol(&p.project_id)? {
        Some(text) => print!("{}", text),
        None => {
            if std::io::stdout().is_terminal() {
                display::progress("No protocol set for this project");
                display::hint(&format!("Set one with `asenix project protocol set {}`", slug));
            }
        }
    }
    Ok(())
}

fn cmd_project_requirements_set(hub: &str, slug: &str, file: Option<&str>) -> Result<()> {
    let token = load_owner_token(hub);
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let reqs_value: serde_json::Value = if let Some(path) = file {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read '{}'", path))?;
        // Parse as either JSON array or requirements.txt (one package per line)
        if raw.trim_start().starts_with('[') {
            serde_json::from_str(&raw).context("invalid JSON array")?
        } else {
            let pkgs: Vec<&str> = raw
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            serde_json::json!(pkgs)
        }
    } else {
        // Fetch current for editor
        let current = api_client.get_requirements(&p.project_id).unwrap_or(serde_json::json!([]));
        let current_str = serde_json::to_string_pretty(&current)?;
        let edited = open_editor(&current_str)?;
        serde_json::from_str(&edited).context("requirements must be a valid JSON array")?
    };

    let spinner = display::Spinner::new("Saving requirements...");
    match api_client.set_requirements(&token, &p.project_id, &reqs_value) {
        Ok(_) => spinner.finish_success("Requirements saved"),
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_requirements_show(hub: &str, slug: &str) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let reqs = api_client.get_requirements(&p.project_id)?;
    if std::io::stdout().is_terminal() {
        match reqs.as_array() {
            Some(arr) if arr.is_empty() => {
                display::progress("No requirements set");
                display::hint(&format!("Set with `asenix project requirements set {}`", slug));
            }
            Some(arr) => {
                for pkg in arr {
                    println!("{}", pkg.as_str().unwrap_or(&pkg.to_string()));
                }
            }
            None => println!("{}", serde_json::to_string_pretty(&reqs)?),
        }
    } else {
        match reqs.as_array() {
            Some(arr) => {
                for pkg in arr {
                    println!("{}", pkg.as_str().unwrap_or(&pkg.to_string()));
                }
            }
            None => println!("{}", reqs),
        }
    }
    Ok(())
}

fn cmd_project_seed_bounty_set(hub: &str, slug: &str, file: Option<&str>) -> Result<()> {
    let token = load_owner_token(hub);
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let bounty: serde_json::Value = if let Some(path) = file {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read '{}'", path))?;
        serde_json::from_str(&raw).context("file must contain valid JSON")?
    } else {
        let current = api_client
            .get_seed_bounty(&p.project_id)
            .unwrap_or(None)
            .unwrap_or(serde_json::json!({}));
        let current_str = serde_json::to_string_pretty(&current)?;
        let edited = open_editor(&current_str)?;
        serde_json::from_str(&edited).context("seed bounty must be valid JSON")?
    };

    let spinner = display::Spinner::new("Saving seed bounty...");
    match api_client.set_seed_bounty(&token, &p.project_id, &bounty) {
        Ok(_) => spinner.finish_success("Seed bounty saved"),
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_seed_bounty_show(hub: &str, slug: &str) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    match api_client.get_seed_bounty(&p.project_id)? {
        Some(bounty) => println!("{}", serde_json::to_string_pretty(&bounty)?),
        None => {
            if std::io::stdout().is_terminal() {
                display::progress("No seed bounty set for this project");
                display::hint(&format!("Set one with `asenix project seed-bounty set {}`", slug));
            }
        }
    }
    Ok(())
}

fn cmd_project_files_list(hub: &str, slug: &str) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let files = match api_client.list_project_files(&p.project_id) {
        Ok(f) => f,
        Err(e) => {
            display::error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    };

    if files.is_empty() {
        if std::io::stdout().is_terminal() {
            display::progress("No files attached to this project");
            display::hint(&format!("Upload with `asenix project files upload {} <path>`", slug));
        }
        return Ok(());
    }

    if std::io::stdout().is_terminal() {
        display::print_table(
            &["Filename", "Size", "Type", "Uploaded"],
            &files
                .iter()
                .map(|f| {
                    vec![
                        f.filename.clone(),
                        format_bytes(f.size_bytes),
                        f.content_type.as_deref().unwrap_or("—").to_string(),
                        f.uploaded_at.chars().take(10).collect(),
                    ]
                })
                .collect::<Vec<_>>(),
        );
    } else {
        for f in &files {
            println!("{}", f.filename);
        }
    }
    Ok(())
}

fn cmd_project_files_upload(hub: &str, slug: &str, filepath: &str, name: Option<&str>) -> Result<()> {
    let token = load_owner_token(hub);
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let path = std::path::Path::new(filepath);
    if !path.exists() {
        display::error(&format!("File not found: {}", filepath));
        std::process::exit(1);
    }

    let filename = name.unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
    });
    let content_type = guess_content_type(filename);
    let data = std::fs::read(path).with_context(|| format!("failed to read '{}'", filepath))?;
    let size = data.len();

    let spinner = display::Spinner::new(&format!("Uploading '{}' ({})...", filename, format_bytes(size as i64)));
    match api_client.upload_project_file(&token, &p.project_id, filename, data, content_type) {
        Ok(_) => spinner.finish_success(&format!("Uploaded '{}'", filename)),
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_files_download(hub: &str, slug: &str, filename: &str, out: Option<&str>) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let spinner = display::Spinner::new(&format!("Downloading '{}'...", filename));
    match api_client.download_project_file(&p.project_id, filename) {
        Ok(bytes) => {
            let dest = out.unwrap_or(filename);
            std::fs::write(dest, &bytes).with_context(|| format!("failed to write '{}'", dest))?;
            spinner.finish_success(&format!("Saved to '{}'", dest));
        }
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_project_files_delete(hub: &str, slug: &str, filename: &str) -> Result<()> {
    let token = load_owner_token(hub);
    let api_client = client::AsenixClient::new(hub);
    let p = resolve_project(&api_client, hub, slug);

    let spinner = display::Spinner::new(&format!("Deleting '{}'...", filename));
    match api_client.delete_project_file(&token, &p.project_id, filename) {
        Ok(_) => spinner.finish_success(&format!("Deleted '{}'", filename)),
        Err(e) => {
            spinner.finish_error(&format!("Failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_domain_install(path: &str) -> Result<()> {
    let src = std::path::Path::new(path);
    if !src.exists() {
        display::error(&format!("Path does not exist: {}", path));
        display::hint("Provide the path to a directory containing domain.toml");
        std::process::exit(1);
    }
    if !src.is_dir() {
        display::error(&format!("Not a directory: {}", path));
        display::hint("A domain pack must be a directory containing domain.toml");
        std::process::exit(1);
    }

    display::progress(&format!("Installing domain pack from {}...", path));

    match domain::install_pack(src) {
        Ok(name) => {
            let dest = config::domain_pack_dir(&name);
            display::success(&format!(
                "Installed domain '{}' → {}",
                name,
                dest.display()
            ));
            display::hint(&format!(
                "Run agents with: asenix agent run --domain {}",
                name
            ));
        }
        Err(e) => {
            display::error(&format!("Install failed: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_domain_list() -> Result<()> {
    match domain::list_packs() {
        Ok(packs) if packs.is_empty() => {
            display::progress("No domain packs installed");
            display::hint("Run `asenix domain install <path>` to install a domain pack");
        }
        Ok(packs) => {
            display::print_table(
                &["Name", "Description", "Files", "Bounty", "Deps"],
                &packs
                    .iter()
                    .map(|p| {
                        vec![
                            p.name.clone(),
                            display::truncate(&p.description, 50),
                            p.file_count.to_string(),
                            if p.has_bounty { "✓" } else { "—" }.to_string(),
                            if p.has_requirements { "✓" } else { "—" }.to_string(),
                        ]
                    })
                    .collect::<Vec<_>>(),
            );
        }
        Err(e) => {
            display::error(&format!("Failed to list domains: {}", e));
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_agent_list() -> Result<()> {
    let agents = config::load_all_agents()?;

    if agents.is_empty() {
        display::progress("No agents registered on this machine");
        display::hint("Run `asenix agent run` to register and launch agents");
        return Ok(());
    }

    display::print_table(
        &["N", "Agent ID", "Hub", "Domain", "Created"],
        &agents
            .iter()
            .map(|(n, cred, _)| {
                vec![
                    n.to_string(),
                    display::truncate(&cred.agent_id, 20),
                    cred.hub.clone(),
                    cred.domain.clone(),
                    cred.created_at.chars().take(10).collect(),
                ]
            })
            .collect::<Vec<_>>(),
    );

    Ok(())
}

fn cmd_agent_stop(project: Option<&str>, only_n: Option<usize>) -> Result<()> {
    let tmp_asenix = std::env::temp_dir().join("asenix");

    // Collect (project_slug, agent_n, pid_path) tuples.
    let candidates: Vec<(String, usize, std::path::PathBuf)> = if let Some(slug) = project {
        collect_pid_paths(&tmp_asenix.join(slug), slug, only_n)
    } else {
        let mut all = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&tmp_asenix) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let slug = entry.file_name().to_string_lossy().into_owned();
                    all.extend(collect_pid_paths(&entry.path(), &slug, only_n));
                }
            }
        }
        all
    };

    if candidates.is_empty() {
        display::progress("No running agents found");
        display::hint("Agents are only stoppable when launched with `agent run -n 2` or more");
        return Ok(());
    }

    let mut stopped = 0usize;
    let mut already_gone = 0usize;

    for (slug, agent_n, pid_path) in &candidates {
        let pid_str = match std::fs::read_to_string(pid_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let pid: u32 = match pid_str.trim().parse() {
            Ok(p) if p > 0 => p,
            _ => continue,
        };

        #[cfg(unix)]
        {
            let ret = unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGTERM) };
            if ret == 0 {
                display::success(&format!(
                    "Stopped agent {} (project: {}, pgid: {})",
                    agent_n, slug, pid
                ));
                stopped += 1;
            } else {
                already_gone += 1;
            }
        }
        #[cfg(not(unix))]
        {
            display::error("agent stop is only supported on Unix");
        }

        // Remove PID file regardless — process is either stopped or already gone.
        let _ = std::fs::remove_file(pid_path);
    }

    if stopped == 0 && already_gone > 0 {
        display::progress(&format!(
            "{} agent(s) were already stopped",
            already_gone
        ));
    }

    Ok(())
}

/// Collect all agent.pid files under `dir/<n>/agent.pid`.
fn collect_pid_paths(
    dir: &std::path::Path,
    slug: &str,
    only_n: Option<usize>,
) -> Vec<(String, usize, std::path::PathBuf)> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return result,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let n: usize = match entry.file_name().to_string_lossy().parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if let Some(only) = only_n {
            if n != only {
                continue;
            }
        }
        let pid_path = path.join("agent.pid");
        if pid_path.exists() {
            result.push((slug.to_string(), n, pid_path));
        }
    }
    result
}

fn cmd_bounty_post(hub: &str, domain: &str) -> Result<()> {
    use std::io::{self, BufRead, Write};

    display::progress(&format!(
        "Creating new bounty (hub: {}, domain: {})...",
        hub, domain
    ));
    println!();

    // Statement
    println!("Statement (empty line to finish):");
    let mut lines = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        display::error("Statement cannot be empty");
        std::process::exit(1);
    }
    let statement = lines.join("\n");

    // Conditions
    println!("\nConditions (key=value, empty line to finish):");
    let mut conditions = serde_json::Map::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        match line.split_once('=') {
            Some((k, v)) => {
                conditions.insert(
                    k.trim().to_string(),
                    serde_json::Value::String(v.trim().to_string()),
                );
            }
            None => display::hint("Format: key=value"),
        }
    }

    // Metrics
    println!("\nMetrics (name=value:unit:direction, e.g. loss=2.5:nats:lower_better, empty to finish):");
    let mut metrics: Vec<serde_json::Value> = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        if let Some((name_val, rest)) = line.split_once(':') {
            if let Some((name, val_str)) = name_val.split_once('=') {
                if let Ok(val) = val_str.trim().parse::<f64>() {
                    let parts: Vec<&str> = rest.splitn(2, ':').collect();
                    let unit = parts.first().map(|s| s.trim()).filter(|s| !s.is_empty());
                    let direction = parts.get(1).map(|s| s.trim()).unwrap_or("neutral");
                    metrics.push(serde_json::json!({
                        "name": name.trim(),
                        "value": val,
                        "unit": unit,
                        "direction": direction,
                    }));
                    continue;
                }
            }
        } else if let Some((name, val_str)) = line.split_once('=') {
            // name=value without unit/direction
            if let Ok(val) = val_str.trim().parse::<f64>() {
                metrics.push(serde_json::json!({ "name": name.trim(), "value": val }));
                continue;
            }
        }
        display::hint("Format: name=value:unit:direction (e.g. accuracy=0.95:percent:higher_better)");
    }

    // Preview
    println!();
    display::divider();
    println!("  {}", "Preview".bold());
    println!("  Type:       bounty");
    println!("  Domain:     {}", domain);
    println!("  Statement:  {}", display::truncate(&statement, 80));
    if !conditions.is_empty() {
        let cond_str: Vec<String> = conditions
            .iter()
            .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or("?")))
            .collect();
        println!("  Conditions: {}", cond_str.join(", "));
    }
    if !metrics.is_empty() {
        println!("  Metrics:    {} item(s)", metrics.len());
    }
    display::divider();
    println!();

    print!("Post? [y/N]: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim().to_lowercase() != "y" {
        println!("Cancelled.");
        return Ok(());
    }

    // Need an agent credential to authenticate the publish call
    let agents = config::load_all_agents()?;
    let (_, agent, _) = agents
        .into_iter()
        .find(|(_, a, _)| a.hub == hub)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no registered agent for hub {}\n  hint: Run `asenix agent run --hub {}` first",
                hub,
                hub
            )
        })?;

    display::progress("Publishing bounty...");
    let api_client = client::AsenixClient::new(hub);
    let result = api_client.mcp_call(
        "publish_atoms",
        serde_json::json!({
            "agent_id": agent.agent_id,
            "api_token": agent.api_token,
            "atoms": [{
                "atom_type": "bounty",
                "domain": domain,
                "statement": statement,
                "conditions": conditions,
                "metrics": metrics,
            }]
        }),
    )?;

    let atom_id = result
        .pointer("/published_atoms/0")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    display::success(&format!("Bounty posted: atom_id={}", atom_id));
    Ok(())
}

fn cmd_bounty_list(hub: &str, domain: Option<&str>) -> Result<()> {
    let api_client = client::AsenixClient::new(hub);
    let spinner = display::Spinner::new("Loading bounties...");

    let mut args = serde_json::json!({ "type": "bounty", "limit": 50 });
    if let Some(d) = domain {
        args["domain"] = serde_json::Value::String(d.to_string());
    }

    let result = match api_client.mcp_call("search_atoms", args) {
        Ok(r) => {
            spinner.finish_success("Loaded");
            r
        }
        Err(e) => {
            spinner.finish_error(&format!("Failed to load bounties: {}", e));
            display::hint("Check that the hub is running: `asenix status`");
            std::process::exit(1);
        }
    };

    let empty = vec![];
    let atoms = result["atoms"].as_array().unwrap_or(&empty);

    if atoms.is_empty() {
        let filter = domain
            .map(|d| format!(" in domain '{}'", d))
            .unwrap_or_default();
        println!("No bounties found{}.", filter);
        display::hint("Post one with `asenix bounty post`");
        return Ok(());
    }

    display::print_table(
        &["ID", "Statement", "Attraction", "Created"],
        &atoms
            .iter()
            .map(|a| {
                vec![
                    display::truncate(a["atom_id"].as_str().unwrap_or(""), 14),
                    display::truncate(a["statement"].as_str().unwrap_or(""), 60),
                    format!(
                        "{:.2}",
                        a.pointer("/pheromone/attraction")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                    ),
                    a["created_at"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(10)
                        .collect(),
                ]
            })
            .collect::<Vec<_>>(),
    );

    Ok(())
}

fn cmd_queue(hub: &str) -> Result<()> {
    use std::io::{self, Write};

    let auth = match config::load_auth() {
        Ok(a) => a,
        Err(_) => {
            display::error("Not logged in");
            display::hint(&format!("Run `asenix login --hub {}`", hub));
            std::process::exit(1);
        }
    };

    let api_client = client::AsenixClient::new(hub);
    display::progress(&format!("Loading review queue from {}...", hub));

    let queue = match api_client.get_review_queue(&auth.token) {
        Ok(q) => q,
        Err(e) => {
            let msg = e.to_string();
            display::error(&format!("Failed to load queue: {}", msg));
            if msg.contains("unauthorized") || msg.contains("expired") {
                display::hint(&format!(
                    "Token may have expired — run `asenix login --hub {}`",
                    hub
                ));
            }
            std::process::exit(1);
        }
    };

    if queue.items.is_empty() {
        display::success("Queue is empty — nothing pending review");
        return Ok(());
    }

    println!("  {} item(s) pending review\n", queue.total);

    let total = queue.items.len();
    let mut approved = 0usize;
    let mut rejected = 0usize;

    for (i, item) in queue.items.iter().enumerate() {
        display::divider();
        println!(
            "[{}/{}] {}  ·  {}  ·  {}",
            i + 1,
            total,
            item.atom_type.cyan(),
            item.domain,
            display::truncate(&item.atom_id, 14)
        );
        println!("  Statement: {}", display::truncate(&item.statement, 72));
        println!("  Author:    {}", display::truncate(&item.author_agent_id, 20));
        println!(
            "  Created:   {}",
            item.created_at.chars().take(19).collect::<String>()
        );
        println!();

        loop {
            print!("[a]pprove / [r]eject / [s]kip > ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            match input.trim() {
                "a" | "approve" => {
                    match api_client.post_review(&auth.token, &item.atom_id, "approve", None) {
                        Ok(_) => {
                            display::success("Approved");
                            approved += 1;
                        }
                        Err(e) => display::error(&format!("Failed: {}", e)),
                    }
                    break;
                }
                "r" | "reject" => {
                    print!("Reason (optional, Enter to skip): ");
                    io::stdout().flush()?;
                    let mut reason = String::new();
                    io::stdin().read_line(&mut reason)?;
                    let reason = reason.trim();
                    let reason_opt = if reason.is_empty() { None } else { Some(reason) };
                    match api_client.post_review(&auth.token, &item.atom_id, "reject", reason_opt) {
                        Ok(_) => {
                            display::success("Rejected");
                            rejected += 1;
                        }
                        Err(e) => display::error(&format!("Failed: {}", e)),
                    }
                    break;
                }
                "s" | "skip" | "" => break,
                _ => display::hint("Enter a, r, or s"),
            }
        }
    }

    println!();
    display::divider();
    println!(
        "  Reviewed {} item(s): {} approved, {} rejected, {} skipped",
        total,
        approved,
        rejected,
        total - approved - rejected
    );
    Ok(())
}

fn cmd_login(hub: &str) -> Result<()> {
    println!("Hub: {}", hub);
    let secret = rpassword::prompt_password("Owner secret: ")
        .context("failed to read password from terminal")?;

    let api_client = client::AsenixClient::new(hub);
    let spinner = display::Spinner::new("Authenticating...");

    match api_client.admin_login(&secret) {
        Ok(token) => {
            spinner.finish_success("Authenticated");
            let expires_at = chrono::Utc::now() + chrono::TimeDelta::hours(24);
            let auth = config::AuthConfig {
                hub: hub.to_string(),
                token,
                expires_at: expires_at.to_rfc3339(),
            };
            config::save_auth(&auth)?;
            display::success(&format!(
                "Logged in until {} UTC",
                expires_at.format("%Y-%m-%d %H:%M:%S")
            ));
            display::hint(&format!(
                "Token saved to {}",
                config::auth_path().display()
            ));
        }
        Err(e) => {
            spinner.finish_error(&format!("Authentication failed: {}", e));
            display::hint("Check the OWNER_SECRET environment variable on your Asenix server");
            std::process::exit(1);
        }
    }
    Ok(())
}

fn cmd_logs(n: Option<usize>) -> Result<()> {
    use std::io::{BufRead, BufReader};

    let logs_dir = config::logs_dir();
    if !logs_dir.exists() {
        display::error("No log directory found");
        display::hint("Run `asenix agent run` to create agents");
        std::process::exit(1);
    }

    // Collect all *.log files sorted by modification time (newest first).
    let mut all_logs: Vec<std::path::PathBuf> = std::fs::read_dir(&logs_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".log")
        })
        .map(|e| e.path())
        .collect();

    all_logs.sort_by(|a, b| {
        let mt_a = a.metadata().and_then(|m| m.modified()).ok();
        let mt_b = b.metadata().and_then(|m| m.modified()).ok();
        mt_b.cmp(&mt_a) // newest first
    });

    if all_logs.is_empty() {
        display::error("No log files found");
        display::hint(&format!("Logs dir: {}", logs_dir.display()));
        std::process::exit(1);
    }

    // If an agent number was given, filter to logs matching `_agent_<n>_`.
    let target_logs: Vec<std::path::PathBuf> = if let Some(n) = n {
        let pattern = format!("_agent_{}_", n);
        let filtered: Vec<_> = all_logs
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .map(|f| f.to_string_lossy().contains(&pattern))
                    .unwrap_or(false)
            })
            .collect();
        if filtered.is_empty() {
            display::error(&format!("No log files found for agent {}", n));
            display::hint(&format!("Logs dir: {}", logs_dir.display()));
            std::process::exit(1);
        }
        filtered
    } else {
        all_logs
    };

    if target_logs.len() == 1 {
        // Single log: tail it.
        let path = &target_logs[0];
        display::progress(&format!("Tailing {} (Ctrl+C to stop)", path.display()));
        let file = std::fs::File::open(path)?;
        let mut reader = BufReader::new(file);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Ok(_) => {
                    // Log lines are raw stream-json; pretty-print them.
                    let trimmed = line.trim_end();
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        // Reuse same display logic as live streaming.
                        print_stream_event(trimmed);
                        let _ = v; // already handled
                    } else {
                        print!("{}", line);
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else {
        // Multiple logs: show a table then multiplex the most recent N.
        println!("  {} logs in {}", target_logs.len(), logs_dir.display());
        println!();
        let rows: Vec<Vec<String>> = target_logs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                vec![
                    (i + 1).to_string(),
                    p.file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ]
            })
            .collect();
        display::print_table(&["#", "File"], &rows);
        println!();
        display::progress(&format!(
            "Multiplexing {} log(s) (Ctrl+C to stop)",
            target_logs.len()
        ));

        let handles: Vec<_> = target_logs
            .into_iter()
            .enumerate()
            .map(|(i, path)| {
                let prefix = path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("agent_{}", i + 1));
                std::thread::spawn(move || {
                    let file = match std::fs::File::open(&path) {
                        Ok(f) => f,
                        Err(e) => {
                            eprintln!("[{}] error opening log: {}", prefix, e);
                            return;
                        }
                    };
                    let mut reader = BufReader::new(file);
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => {
                                std::thread::sleep(std::time::Duration::from_millis(100))
                            }
                            Ok(_) => print!("[{}] {}", prefix, line),
                            Err(_) => break,
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            let _ = h.join();
        }
    }

    Ok(())
}

fn cmd_reset(hub: &str) -> Result<()> {
    use std::io::{self, Write};

    // Suppress unused warning — hub is part of the interface for consistency but not needed
    let _ = hub;

    let hostname = config::hostname();
    let creds_dir = config::agent_creds_dir(&hostname);
    let logs_dir = config::logs_dir();
    let auth_path = config::auth_path();

    let cred_count = if creds_dir.exists() {
        std::fs::read_dir(&creds_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".toml"))
            .count()
    } else {
        0
    };
    let log_count = if logs_dir.exists() {
        std::fs::read_dir(&logs_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".log"))
            .count()
    } else {
        0
    };
    let has_auth = auth_path.exists();

    if cred_count == 0 && log_count == 0 && !has_auth {
        println!("Nothing to delete — local state is already clean.");
        return Ok(());
    }

    display::progress("This will delete:");
    if cred_count > 0 {
        println!(
            "  - {} credential file(s) in {}",
            cred_count,
            creds_dir.display()
        );
    }
    if log_count > 0 {
        println!("  - {} log file(s) in {}", log_count, logs_dir.display());
    }
    if has_auth {
        println!("  - auth token in {}", auth_path.display());
    }
    println!();

    print!("Are you sure? [y/N]: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim().to_lowercase() != "y" {
        println!("Cancelled.");
        return Ok(());
    }

    display::progress("Deleting...");
    let deleted_creds = config::delete_host_data(&hostname)?;
    let deleted_logs = config::delete_logs()?;
    if has_auth {
        std::fs::remove_file(&auth_path)?;
    }

    display::success(&format!(
        "Reset complete: {} credential(s), {} log(s){} deleted",
        deleted_creds,
        deleted_logs,
        if has_auth { ", auth token" } else { "" }
    ));
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_up() {
        let cli = Cli::try_parse_from(["asenix", "up"]).unwrap();
        assert!(matches!(cli.command, Commands::Up));
    }

    #[test]
    fn cli_parses_down() {
        let cli = Cli::try_parse_from(["asenix", "down"]).unwrap();
        assert!(matches!(cli.command, Commands::Down));
    }

    #[test]
    fn cli_parses_status_default_hub() {
        let cli = Cli::try_parse_from(["asenix", "status"]).unwrap();
        match cli.command {
            Commands::Status { hub } => assert_eq!(hub, "http://localhost:3000"),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_status_custom_hub() {
        let cli =
            Cli::try_parse_from(["asenix", "status", "--hub", "http://prod:3000"]).unwrap();
        match cli.command {
            Commands::Status { hub } => assert_eq!(hub, "http://prod:3000"),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_agent_run_requires_project() {
        // --project is required; omitting it must fail
        assert!(Cli::try_parse_from(["asenix", "agent", "run"]).is_err());
    }

    #[test]
    fn cli_parses_agent_run_with_project() {
        let cli =
            Cli::try_parse_from(["asenix", "agent", "run", "--project", "cifar10-resnet"]).unwrap();
        match cli.command {
            Commands::Agent {
                subcommand: AgentCommands::Run { project, n, .. },
            } => {
                assert_eq!(project, "cifar10-resnet");
                assert_eq!(n, 1); // default
            }
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_agent_run_with_n() {
        let cli =
            Cli::try_parse_from(["asenix", "agent", "run", "--project", "ml", "--n", "3"])
                .unwrap();
        match cli.command {
            Commands::Agent {
                subcommand: AgentCommands::Run { project, n, .. },
            } => {
                assert_eq!(project, "ml");
                assert_eq!(n, 3);
            }
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_domain_install() {
        let cli =
            Cli::try_parse_from(["asenix", "domain", "install", "/tmp/my-pack"]).unwrap();
        match cli.command {
            Commands::Domain {
                subcommand: DomainCommands::Install { path },
            } => assert_eq!(path, "/tmp/my-pack"),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_domain_list() {
        let cli = Cli::try_parse_from(["asenix", "domain", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Domain {
                subcommand: DomainCommands::List
            }
        ));
    }

    #[test]
    fn cli_parses_agent_list() {
        let cli = Cli::try_parse_from(["asenix", "agent", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Agent {
                subcommand: AgentCommands::List
            }
        ));
    }

    #[test]
    fn cli_parses_bounty_post() {
        let cli = Cli::try_parse_from(["asenix", "bounty", "post", "--domain", "bio"]).unwrap();
        match cli.command {
            Commands::Bounty {
                subcommand: BountyCommands::Post { domain, .. },
            } => assert_eq!(domain, "bio"),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_bounty_list_with_domain() {
        let cli =
            Cli::try_parse_from(["asenix", "bounty", "list", "--domain", "physics"]).unwrap();
        match cli.command {
            Commands::Bounty {
                subcommand: BountyCommands::List { domain, .. },
            } => assert_eq!(domain.as_deref(), Some("physics")),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_bounty_list_without_domain() {
        let cli = Cli::try_parse_from(["asenix", "bounty", "list"]).unwrap();
        match cli.command {
            Commands::Bounty {
                subcommand: BountyCommands::List { domain, .. },
            } => assert!(domain.is_none()),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_queue() {
        let cli = Cli::try_parse_from(["asenix", "queue"]).unwrap();
        assert!(matches!(cli.command, Commands::Queue { .. }));
    }

    #[test]
    fn cli_parses_login() {
        let cli = Cli::try_parse_from(["asenix", "login"]).unwrap();
        assert!(matches!(cli.command, Commands::Login { .. }));
    }

    #[test]
    fn cli_parses_logs_with_n() {
        let cli = Cli::try_parse_from(["asenix", "logs", "3"]).unwrap();
        match cli.command {
            Commands::Logs { n } => assert_eq!(n, Some(3)),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_logs_without_n() {
        let cli = Cli::try_parse_from(["asenix", "logs"]).unwrap();
        match cli.command {
            Commands::Logs { n } => assert!(n.is_none()),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_parses_reset() {
        let cli = Cli::try_parse_from(["asenix", "reset"]).unwrap();
        assert!(matches!(cli.command, Commands::Reset { .. }));
    }

    #[test]
    fn unknown_subcommand_fails() {
        assert!(Cli::try_parse_from(["asenix", "frobulate"]).is_err());
    }

    #[test]
    fn cli_domain_install_requires_path() {
        assert!(Cli::try_parse_from(["asenix", "domain", "install"]).is_err());
    }
}

