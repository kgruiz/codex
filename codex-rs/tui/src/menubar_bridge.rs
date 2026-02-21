use codex_core::AuthManager;
use codex_core::ThreadManager;
use codex_core::config::Config;
use codex_core::protocol::EventMsg;
use std::sync::Arc;
use toml::Value as TomlValue;

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use codex_codexd::producer::CodexdProducerClient;
    use codex_codexd::producer::RuntimeMetadata;
    use codex_codexd::protocol::HubNotification;
    use serde_json::json;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::path::PathBuf;

    pub struct MenuBarBridge {
        producer: CodexdProducerClient,
        active_turns: HashMap<String, String>,
        turn_start_order: Vec<String>,
        known_turn_keys: HashSet<String>,
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

            Some(Self {
                producer,
                active_turns: HashMap::new(),
                turn_start_order: Vec::new(),
                known_turn_keys: HashSet::new(),
            })
        }

        pub fn publish_event(&mut self, event: &EventMsg) {
            let mut notifications = Vec::new();
            match event {
                EventMsg::ItemStarted(item) => {
                    notifications.extend(
                        self.ensure_turn_started(item.thread_id.to_string(), item.turn_id.clone()),
                    );
                    notifications.push(HubNotification {
                        method: "item/started".to_string(),
                        params: Some(json!({
                            "threadId": item.thread_id,
                            "turnId": item.turn_id,
                            "item": item.item,
                        })),
                    });
                }
                EventMsg::ItemCompleted(item) => {
                    notifications.push(HubNotification {
                        method: "item/completed".to_string(),
                        params: Some(json!({
                            "threadId": item.thread_id,
                            "turnId": item.turn_id,
                            "item": item.item,
                        })),
                    });
                }
                EventMsg::ProgressTrace(trace) => {
                    if trace.state == codex_core::protocol::ProgressTraceState::Started {
                        notifications.extend(self.ensure_turn_started(
                            trace.thread_id.to_string(),
                            trace.turn_id.clone(),
                        ));
                    }
                    notifications.push(HubNotification {
                        method: "turn/progressTrace".to_string(),
                        params: Some(json!({
                            "threadId": trace.thread_id,
                            "turnId": trace.turn_id,
                            "category": trace.category,
                            "state": trace.state,
                            "label": trace.label,
                        })),
                    });
                }
                EventMsg::TurnComplete(_) => {
                    notifications.extend(self.complete_latest_turn());
                }
                EventMsg::Error(error) => {
                    notifications.push(HubNotification {
                        method: "error".to_string(),
                        params: Some(json!({
                            "error": {
                                "message": error.message,
                            },
                            "willRetry": false,
                        })),
                    });
                }
                _ => {}
            }

            for notification in notifications {
                let producer = self.producer.clone();
                tokio::spawn(async move {
                    producer.publish_hub_notification(notification).await;
                });
            }
        }

        fn ensure_turn_started(
            &mut self,
            thread_id: String,
            turn_id: String,
        ) -> Vec<HubNotification> {
            let key = format!("{thread_id}:{turn_id}");
            if self.known_turn_keys.contains(&key) {
                return Vec::new();
            }

            self.known_turn_keys.insert(key.clone());
            self.active_turns.insert(key.clone(), thread_id.clone());
            self.turn_start_order.push(key);

            vec![HubNotification {
                method: "turn/started".to_string(),
                params: Some(json!({
                    "threadId": thread_id,
                    "turn": {
                        "id": turn_id,
                        "status": "inProgress",
                    }
                })),
            }]
        }

        fn complete_latest_turn(&mut self) -> Vec<HubNotification> {
            while let Some(key) = self.turn_start_order.pop() {
                let Some(thread_id) = self.active_turns.remove(&key) else {
                    continue;
                };
                let Some((_, turn_id)) = key.split_once(':') else {
                    continue;
                };
                return vec![HubNotification {
                    method: "turn/completed".to_string(),
                    params: Some(json!({
                        "threadId": thread_id,
                        "turn": {
                            "id": turn_id,
                            "status": "completed",
                        }
                    })),
                }];
            }

            Vec::new()
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

        pub fn publish_event(&mut self, _event: &EventMsg) {}

        pub async fn shutdown(self) {}
    }
}

pub use imp::MenuBarBridge;
