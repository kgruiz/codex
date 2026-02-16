use codex_core::AuthManager;
use codex_core::ThreadManager;
use codex_core::config::Config;
use std::sync::Arc;
use toml::Value as TomlValue;

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use chrono::SecondsFormat;
    use chrono::Utc;
    use codex_app_server::EmbeddedAppServerHandle;
    use codex_app_server::start_embedded_websocket_server;
    use serde::Serialize;
    use std::net::Ipv4Addr;
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use tokio::fs;
    use tracing::warn;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EndpointRecord {
        schema_version: u32,
        pid: u32,
        endpoint_url: String,
        started_at: String,
        codex_version: String,
    }

    pub struct MenuBarBridge {
        endpoint_file: PathBuf,
        handle: EmbeddedAppServerHandle,
    }

    impl MenuBarBridge {
        pub async fn start(
            codex_linux_sandbox_exe: Option<PathBuf>,
            config: Arc<Config>,
            auth_manager: Arc<AuthManager>,
            thread_manager: Arc<ThreadManager>,
            cli_overrides: Vec<(String, TomlValue)>,
        ) -> Option<Self> {
            let bind_address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
            let handle = match start_embedded_websocket_server(
                codex_linux_sandbox_exe,
                config.clone(),
                auth_manager,
                thread_manager,
                cli_overrides,
                bind_address,
            )
            .await
            {
                Ok(handle) => handle,
                Err(err) => {
                    warn!("failed to start embedded app-server for menu bar: {err}");
                    return None;
                }
            };

            let endpoint_dir = config
                .codex_home
                .join("runtime")
                .join("menubar")
                .join("endpoints");
            if let Err(err) = fs::create_dir_all(&endpoint_dir).await {
                warn!(
                    "failed to create menu bar runtime endpoint dir {}: {err}",
                    endpoint_dir.display()
                );
                let _ = handle.shutdown().await;
                return None;
            }

            let pid = std::process::id();
            let endpoint_file = endpoint_dir.join(format!("{pid}.json"));
            let tmp_file = endpoint_dir.join(format!("{pid}.json.tmp"));
            let endpoint_record = EndpointRecord {
                schema_version: 1,
                pid,
                endpoint_url: handle.websocket_url().to_string(),
                started_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                codex_version: env!("CARGO_PKG_VERSION").to_string(),
            };

            let payload = match serde_json::to_vec_pretty(&endpoint_record) {
                Ok(payload) => payload,
                Err(err) => {
                    warn!("failed to serialize menu bar endpoint record: {err}");
                    let _ = handle.shutdown().await;
                    return None;
                }
            };

            if let Err(err) = fs::write(&tmp_file, payload).await {
                warn!(
                    "failed to write menu bar endpoint temp file {}: {err}",
                    tmp_file.display()
                );
                let _ = handle.shutdown().await;
                return None;
            }

            if let Err(err) = fs::rename(&tmp_file, &endpoint_file).await {
                warn!(
                    "failed to publish menu bar endpoint file {}: {err}",
                    endpoint_file.display()
                );
                let _ = fs::remove_file(&tmp_file).await;
                let _ = handle.shutdown().await;
                return None;
            }

            Some(Self {
                endpoint_file,
                handle,
            })
        }

        pub async fn shutdown(self) {
            if let Err(err) = fs::remove_file(&self.endpoint_file).await
                && err.kind() != std::io::ErrorKind::NotFound
            {
                warn!(
                    "failed to remove menu bar endpoint file {}: {err}",
                    self.endpoint_file.display()
                );
            }

            if let Err(err) = self.handle.shutdown().await {
                warn!("failed to shut down embedded app-server for menu bar: {err}");
            }
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
