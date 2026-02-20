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
    use codex_app_server::start_embedded_uds_server;
    use serde::Serialize;
    use std::path::Path;
    use std::path::PathBuf;
    use tokio::fs;
    use tracing::warn;
    use uuid::Uuid;

    const ENDPOINT_SCHEMA_VERSION: u32 = 3;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EndpointRecord {
        schema_version: u32,
        pid: u32,
        socket_path: String,
        auth_token: String,
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
            let runtime_dir = config.codex_home.join("runtime").join("menubar");
            let endpoint_dir = runtime_dir.join("endpoints");
            let socket_dir = runtime_dir.join("sockets");
            if let Err(err) = fs::create_dir_all(&endpoint_dir).await {
                warn!(
                    "failed to create menu bar runtime endpoint dir {}: {err}",
                    endpoint_dir.display()
                );
                return None;
            }
            if let Err(err) = fs::create_dir_all(&socket_dir).await {
                warn!(
                    "failed to create menu bar runtime socket dir {}: {err}",
                    socket_dir.display()
                );
                return None;
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    fs::set_permissions(&runtime_dir, std::fs::Permissions::from_mode(0o700)).await;
                let _ = fs::set_permissions(&endpoint_dir, std::fs::Permissions::from_mode(0o700))
                    .await;
                let _ =
                    fs::set_permissions(&socket_dir, std::fs::Permissions::from_mode(0o700)).await;
            }

            let socket_id = Uuid::new_v4().to_string();
            let socket_path = socket_dir.join(format!("{socket_id}.sock"));
            let auth_token = Uuid::new_v4().to_string();
            let handle = match start_embedded_uds_server(
                codex_linux_sandbox_exe,
                config.clone(),
                auth_manager,
                thread_manager,
                cli_overrides,
                socket_path.clone(),
            )
            .await
            {
                Ok(handle) => handle,
                Err(err) => {
                    warn!("failed to start embedded app-server for menu bar: {err}");
                    return None;
                }
            };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).await;
            }

            let pid = std::process::id();
            let endpoint_file = endpoint_dir.join(format!("{pid}.json"));
            let tmp_file = endpoint_dir.join(format!("{pid}.json.tmp"));
            let endpoint_record = EndpointRecord {
                schema_version: ENDPOINT_SCHEMA_VERSION,
                pid,
                socket_path: socket_path.to_string_lossy().to_string(),
                auth_token,
                started_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                codex_version: env!("CARGO_PKG_VERSION").to_string(),
            };

            if let Err(err) =
                write_endpoint_record(&endpoint_file, &tmp_file, &endpoint_record).await
            {
                warn!(
                    "failed to publish menu bar endpoint file {}: {err}",
                    endpoint_file.display()
                );
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

    async fn write_endpoint_record(
        endpoint_file: &Path,
        tmp_file: &Path,
        endpoint_record: &EndpointRecord,
    ) -> Result<(), String> {
        let payload = serde_json::to_vec_pretty(endpoint_record)
            .map_err(|err| format!("failed to serialize menu bar endpoint record: {err}"))?;

        fs::write(tmp_file, payload)
            .await
            .map_err(|err| format!("failed to write temp file {}: {err}", tmp_file.display()))?;

        if let Err(err) = fs::rename(tmp_file, endpoint_file).await {
            let _ = fs::remove_file(tmp_file).await;
            return Err(format!(
                "failed to rename temp file {} -> {}: {err}",
                tmp_file.display(),
                endpoint_file.display()
            ));
        }

        Ok(())
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
