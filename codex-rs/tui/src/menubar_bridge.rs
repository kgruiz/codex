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
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::Duration;
    use tokio::fs;
    use tokio::sync::oneshot;
    use tokio::task::JoinHandle;
    use tokio::time::MissedTickBehavior;
    use tracing::warn;

    const ENDPOINT_SCHEMA_VERSION: u32 = 2;
    const HEARTBEAT_INTERVAL_SECS: u64 = 5;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EndpointRecord {
        schema_version: u32,
        pid: u32,
        endpoint_url: String,
        started_at: String,
        last_heartbeat_at: String,
        codex_version: String,
    }

    pub struct MenuBarBridge {
        endpoint_file: PathBuf,
        heartbeat_stop_tx: Option<oneshot::Sender<()>>,
        heartbeat_task: JoinHandle<()>,
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
            let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            let endpoint_record = EndpointRecord {
                schema_version: ENDPOINT_SCHEMA_VERSION,
                pid,
                endpoint_url: handle.websocket_url().to_string(),
                started_at: now.clone(),
                last_heartbeat_at: now,
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

            let heartbeat_endpoint_file = endpoint_file.clone();
            let heartbeat_tmp_file = tmp_file.clone();
            let mut heartbeat_record = endpoint_record;
            let (heartbeat_stop_tx, mut heartbeat_stop_rx) = oneshot::channel();
            let heartbeat_task = tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
                interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                interval.tick().await;

                loop {
                    tokio::select! {
                        _ = &mut heartbeat_stop_rx => {
                            break;
                        }
                        _ = interval.tick() => {
                            heartbeat_record.last_heartbeat_at =
                                Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
                            if let Err(err) = write_endpoint_record(
                                &heartbeat_endpoint_file,
                                &heartbeat_tmp_file,
                                &heartbeat_record
                            ).await {
                                warn!(
                                    "failed to refresh menu bar endpoint heartbeat {}: {err}",
                                    heartbeat_endpoint_file.display()
                                );
                            }
                        }
                    }
                }
            });

            Some(Self {
                endpoint_file,
                heartbeat_stop_tx: Some(heartbeat_stop_tx),
                heartbeat_task,
                handle,
            })
        }

        pub async fn shutdown(mut self) {
            if let Some(stop_tx) = self.heartbeat_stop_tx.take() {
                let _ = stop_tx.send(());
            }

            if let Err(err) = self.heartbeat_task.await {
                warn!("menu bar heartbeat task join failure: {err}");
            }

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
