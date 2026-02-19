use std::env;

/// Iron-bunny specific configuration from environment variables.
///
/// Keeps all existing `BRUNNYLOL_*` env vars working.
/// New `IRON_BUNNY_*` vars for new features.
#[derive(Debug, Clone)]
pub struct IronBunnyConfig {
    /// Path to an external config file (optional, hot-reloaded)
    pub config_path: Option<String>,
    /// Run in development mode (mock Docker, fixture data)
    pub dev_mode: bool,
    /// Path to mock containers YAML (dev mode only)
    pub mock_containers_path: Option<String>,
    /// Listen address (fallback to BRUNNYLOL_PORT)
    pub listen_addr: Option<String>,
}

impl IronBunnyConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let dev_mode = env::var("IRON_BUNNY_DEV_MODE")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        let config_path = env::var("IRON_BUNNY_CONFIG_PATH").ok();

        let mock_containers_path = env::var("IRON_BUNNY_MOCK_CONTAINERS").ok().or_else(|| {
            if dev_mode {
                Some("dev-fixtures/mock-containers.yaml".to_string())
            } else {
                None
            }
        });

        let listen_addr = env::var("IRON_BUNNY_LISTEN_ADDR").ok();

        IronBunnyConfig {
            config_path,
            dev_mode,
            mock_containers_path,
            listen_addr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn clear_env_vars() {
        env::remove_var("IRON_BUNNY_DEV_MODE");
        env::remove_var("IRON_BUNNY_CONFIG_PATH");
        env::remove_var("IRON_BUNNY_MOCK_CONTAINERS");
        env::remove_var("IRON_BUNNY_LISTEN_ADDR");
    }

    #[test]
    fn test_defaults() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();

        let config = IronBunnyConfig::from_env();
        assert!(!config.dev_mode);
        assert!(config.config_path.is_none());
        assert!(config.mock_containers_path.is_none());
        assert!(config.listen_addr.is_none());
    }

    #[test]
    fn test_dev_mode() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();

        env::set_var("IRON_BUNNY_DEV_MODE", "true");

        let config = IronBunnyConfig::from_env();
        assert!(config.dev_mode);
        // Dev mode should default mock containers path
        assert_eq!(
            config.mock_containers_path,
            Some("dev-fixtures/mock-containers.yaml".to_string())
        );

        clear_env_vars();
    }

    #[test]
    fn test_custom_config() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();

        env::set_var("IRON_BUNNY_CONFIG_PATH", "/custom/config.yaml");
        env::set_var("IRON_BUNNY_LISTEN_ADDR", "127.0.0.1:3001");

        let config = IronBunnyConfig::from_env();
        assert_eq!(
            config.config_path,
            Some("/custom/config.yaml".to_string())
        );
        assert_eq!(
            config.listen_addr,
            Some("127.0.0.1:3001".to_string())
        );

        clear_env_vars();
    }
}
