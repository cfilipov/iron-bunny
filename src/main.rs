use brunnylol::config::app_config::IronBunnyConfig;
use brunnylol::coordinator::{self, CoordinatorConfig};
use brunnylol::docker;
use brunnylol::watcher;
use tracing::{error, info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    // Load iron-bunny specific config from env
    let ib_config = IronBunnyConfig::from_env();

    if ib_config.dev_mode {
        info!("DEVELOPMENT MODE ENABLED");
        info!("  Docker socket: DISABLED (using fixture data)");
    }

    // Parse CLI arguments for port
    let matches = clap::Command::new("Brunnylol")
        .arg(
            clap::Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PORT")
                .help("Port to listen on (default: 8000, env: BRUNNYLOL_PORT)"),
        )
        .get_matches();

    // Priority: IRON_BUNNY_LISTEN_ADDR > CLI > BRUNNYLOL_PORT env > Default
    let env_port = std::env::var("BRUNNYLOL_PORT").ok();
    let cli_port = matches.get_one::<String>("port").map(|s| s.as_str());

    let addr = if let Some(listen_addr) = &ib_config.listen_addr {
        listen_addr.clone()
    } else {
        let port = cli_port
            .or(env_port.as_deref())
            .unwrap_or("8000");
        format!("0.0.0.0:{}", port)
    };

    // Create Docker client (skip in dev mode)
    let docker_client = if !ib_config.dev_mode {
        match docker::client::create_docker_client() {
            Ok(client) => {
                info!("Docker client connected");
                Some(client)
            }
            Err(e) => {
                info!("Docker socket not available ({}), running without Docker integration", e);
                None
            }
        }
    } else {
        info!("Skipping Docker client creation (dev mode)");
        None
    };

    // Create the router (this also initializes DB, seeds bookmarks, etc.)
    let (app, state) = brunnylol::create_router().await;

    // Set up rebuild channel
    let (rebuild_tx, rebuild_rx) = tokio::sync::mpsc::channel::<()>(16);

    // Store the rebuild_tx in state so the API /api/reload endpoint can trigger rebuilds
    {
        let mut tx = state.rebuild_tx.write().await;
        *tx = Some(rebuild_tx.clone());
    }

    // Build coordinator config
    let coord_config = CoordinatorConfig {
        config_path: ib_config.config_path.clone(),
        dev_mode: ib_config.dev_mode,
        mock_containers_path: ib_config.mock_containers_path.clone(),
    };

    // Run initial full rebuild to load Docker labels + merge with DB bookmarks
    if let Err(e) = coordinator::full_rebuild(&state, docker_client.as_ref(), &coord_config).await {
        error!("Initial registry rebuild failed: {}", e);
    }

    // Spawn config file watcher if a config path is set
    if let Some(ref config_path) = ib_config.config_path {
        let path = config_path.clone();
        let tx = rebuild_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = watcher::watch_config_file(path, tx).await {
                error!("Config file watcher failed: {}", e);
            }
        });
        info!("Config file watcher started for: {}", config_path);
    }

    // Spawn Docker event watcher (only in non-dev mode with a Docker client)
    if let Some(ref client) = docker_client {
        let client_clone = client.clone();
        let tx = rebuild_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = docker::client::watch_docker_events(client_clone, tx).await {
                error!("Docker event watcher failed: {}", e);
            }
        });
        info!("Docker event watcher started");
    }

    // Spawn rebuild coordinator loop
    let coord_state = state.clone();
    let coord_docker = docker_client.clone();
    let coord_config2 = CoordinatorConfig {
        config_path: ib_config.config_path.clone(),
        dev_mode: ib_config.dev_mode,
        mock_containers_path: ib_config.mock_containers_path.clone(),
    };
    tokio::spawn(async move {
        coordinator::run_rebuild_loop(coord_state, coord_docker, coord_config2, rebuild_rx).await;
    });
    info!("Rebuild coordinator started");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on http://{}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
