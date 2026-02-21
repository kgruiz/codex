use codex_core::AuthManager;
use codex_core::ThreadManager;
use codex_core::config::Config;
use std::sync::Arc;
use toml::Value as TomlValue;

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use codex_codexd::producer::CodexdProducerClient;
    use codex_codexd::producer::RuntimeMetadata;
    use std::path::PathBuf;

    pub struct MenuBarBridge {
        producer: CodexdProducerClient,
    }

    impl MenuBarBridge {
        pub async fn start(
            _codex_linux_sandbox_exe: Option<PathBuf>,
            config: Arc<Config>,
            _auth_manager: Arc<AuthManager>,
            _thread_manager: Arc<ThreadManager>,
            _cli_overrides: Vec<(String, TomlValue)>,
        ) -> Option<Self> {
            let producer = CodexdProducerClient::spawn(
                config.codex_home.as_path(),
                RuntimeMetadata {
                    runtime_id: format!("pid:{}", std::process::id()),
                    pid: Some(std::process::id()),
                    session_source: Some("cli".to_string()),
                    cwd: Some(config.cwd.to_string_lossy().into_owned()),
                    display_name: Some("codex-tui".to_string()),
                },
            );

            Some(Self { producer })
        }

        pub async fn shutdown(self) {
            self.producer.shutdown().await;
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;
    use std::path::PathBuf;

    pub struct MenuBarBridge;

    impl MenuBarBridge {
        pub async fn start(
            _codex_linux_sandbox_exe: Option<PathBuf>,
            _config: Arc<Config>,
            _auth_manager: Arc<AuthManager>,
            _thread_manager: Arc<ThreadManager>,
            _cli_overrides: Vec<(String, TomlValue)>,
        ) -> Option<Self> {
            None
        }

        pub async fn shutdown(self) {}
    }
}

pub use imp::MenuBarBridge;
