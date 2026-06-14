use std::cell::RefCell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use js_sys;
use serde::{Deserialize, Serialize};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, MessageEvent, Request, RequestInit, RequestMode, Response, WebSocket};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigUpdatePayload {
    pub command: String,
    pub target_peer_id: u16,
    pub alternative_peer_id: u16,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum AgentState {
    Active,
    Healing,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MeshChannelState {
    pub peer_id: u16,
    pub nonce: u64,
    pub consecutive_failures: u8,
    pub is_active: bool,
}

#[derive(Serialize, Deserialize)]
pub struct MeshPulsePayload {
    pub status: String,
    pub agent: u16,
    pub active_mesh_neighbors: Vec<u16>,
    #[serde(alias = "target")]
    pub report_target: u16,
    pub attempt: u8,
}

pub struct AgentMeshContext {
    pub channels: Vec<MeshChannelState>,
    pub state: AgentState,
    pub max_failures: u8,
}

fn is_local_endpoint(url: &str) -> bool {
    let Ok(parsed_url) = web_sys::Url::new(url) else {
        return false;
    };

    let hostname = parsed_url.hostname();
    hostname == "127.0.0.1" || hostname == "localhost"
}

#[wasm_bindgen]
pub struct FiberAgent {
    id: u16,
    context: Rc<RefCell<AgentMeshContext>>,
    ws_handle: Rc<RefCell<Option<WebSocket>>>,
    active_closure: Rc<RefCell<Option<Closure<dyn FnMut(MessageEvent)>>>>,
}

#[wasm_bindgen]
impl FiberAgent {
    #[wasm_bindgen(constructor)]
    pub fn new(id: u16, initial_neighbors: Vec<u16>, max_failures: u8) -> FiberAgent {
        let channels = initial_neighbors
            .into_iter()
            .map(|peer_id| MeshChannelState {
                peer_id,
                nonce: 0,
                consecutive_failures: 0,
                is_active: true,
            })
            .collect();

        FiberAgent {
            id,
            context: Rc::new(RefCell::new(AgentMeshContext {
                channels,
                state: AgentState::Active,
                max_failures,
            })),
            ws_handle: Rc::new(RefCell::new(None)),
            active_closure: Rc::new(RefCell::new(None)),
        }
    }

    pub fn get_id(&self) -> u16 {
        self.id
    }

    pub fn get_state_string(&self) -> String {
        format!("{:?}", self.context.borrow().state)
    }

    pub fn get_active_neighbor_count(&self) -> usize {
        self.context
            .borrow()
            .channels
            .iter()
            .filter(|c| c.is_active)
            .count()
    }

    pub fn initialize_websocket_link(&self, ws_url: String) -> Result<(), JsValue> {
        if !is_local_endpoint(&ws_url) {
            return Err(JsValue::from_str(
                "WebSocket URL must target localhost or 127.0.0.1",
            ));
        }

        if let Some(old_ws) = self.ws_handle.borrow_mut().take() {
            old_ws.close()?;
        }
        *self.active_closure.borrow_mut() = None;

        let ws = WebSocket::new(&format!(
            "{}/ws/{}",
            ws_url.trim_end_matches('/'),
            self.id
        ))?;
        let ctx_clone = self.context.clone();

        let onmessage_callback = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let Some(txt) = e.data().as_string() else {
                return;
            };

            let Ok(parsed_cmd) = serde_json::from_str::<ConfigUpdatePayload>(&txt) else {
                return;
            };

            if parsed_cmd.command == "MESH_CHANNEL_HOT_SWAP" {
                let mut ctx = ctx_clone.borrow_mut();
                if let Some(ch) = ctx
                    .channels
                    .iter_mut()
                    .find(|c| c.peer_id == parsed_cmd.target_peer_id)
                {
                    ch.peer_id = parsed_cmd.alternative_peer_id;
                    ch.consecutive_failures = 0;
                    ch.is_active = true;
                    ctx.state = AgentState::Active;
                    web_sys::console::info_1(&JsValue::from_str(
                        "[MESH] Re-routed broken mesh path.",
                    ));
                }
            }
        });

        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        *self.active_closure.borrow_mut() = Some(onmessage_callback);
        *self.ws_handle.borrow_mut() = Some(ws);

        Ok(())
    }

    pub async fn start_mesh_heartbeat_loop(self, tick_rate_ms: u32, mfa_http_url: String) {
        if !is_local_endpoint(&mfa_http_url) {
            web_sys::console::error_1(&JsValue::from_str(
                "Telemetry URL must target localhost or 127.0.0.1",
            ));
            return;
        }

        web_sys::console::log_1(&JsValue::from_str(&format!(
            "[FA-{}] Mesh heartbeat loop active. Telemetry: {}",
            self.id, mfa_http_url
        )));

        loop {
            let alert_json = {
                let mut ctx = self.context.borrow_mut();

                if ctx.state == AgentState::Healing {
                    None
                } else {
                    let channels_len = ctx.channels.len();
                    let mut alert: Option<String> = None;

                    let max_failures = ctx.max_failures;
                    for i in 0..channels_len {
                        if !ctx.channels[i].is_active {
                            continue;
                        }

                        let peer_online = js_sys::Math::random() > 0.05;
                        let channel = &mut ctx.channels[i];

                        if peer_online {
                            channel.consecutive_failures = 0;
                            channel.nonce += 1;
                            web_sys::console::log_1(&JsValue::from_str(&format!(
                                "⚡ [FA-{}] Mesh Link Verified with FA-{}. Nonce: {}",
                                self.id, channel.peer_id, channel.nonce
                            )));
                        } else {
                            channel.consecutive_failures += 1;
                            web_sys::console::warn_1(&JsValue::from_str(&format!(
                                "⚠️ [FA-{}] Mesh link drop warning to FA-{}. Failure index: {}",
                                self.id, channel.peer_id, channel.consecutive_failures
                            )));

                            if channel.consecutive_failures >= max_failures {
                                let report_target = channel.peer_id;
                                let attempt = channel.consecutive_failures;
                                channel.is_active = false;
                                ctx.state = AgentState::Healing;

                                let alert_payload = MeshPulsePayload {
                                    status: "ALERT_MFA_NODE_DROPPED".to_string(),
                                    agent: self.id,
                                    active_mesh_neighbors: ctx
                                        .channels
                                        .iter()
                                        .filter(|c| c.is_active)
                                        .map(|c| c.peer_id)
                                        .collect(),
                                    report_target,
                                    attempt,
                                };

                                alert = Some(serde_json::to_string(&alert_payload).unwrap());
                                break;
                            }
                        }
                    }

                    alert
                }
            };

            if let Some(json_alert) = alert_json {
                match dispatch_http_alert(&mfa_http_url, json_alert).await {
                    Ok(_) => web_sys::console::log_1(&JsValue::from_str(
                        "[SYSTEM] MFA accepted mesh escalation packet.",
                    )),
                    Err(e) => web_sys::console::error_1(&JsValue::from_str(&format!(
                        "[NETWORK ERROR] MFA unreachable: {:?}",
                        e
                    ))),
                }
            }

            TimeoutFuture::new(tick_rate_ms).await;
        }
    }
}

async fn dispatch_http_alert(url: &str, json_payload: String) -> Result<JsValue, JsValue> {
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&JsValue::from_str(&json_payload));

    let headers = Headers::new()?;
    headers.set("Content-Type", "application/json")?;
    opts.set_headers(&headers);

    let window =
        web_sys::window().ok_or_else(|| JsValue::from_str("Global Window Object Unreachable"))?;
    let request = Request::new_with_str_and_init(url, &opts)?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;

    if resp.status() == 202 {
        Ok(JsValue::from_str("ACCEPTED"))
    } else {
        Err(JsValue::from_str(&format!(
            "MFA returned invalid status code: {}",
            resp.status()
        )))
    }
}
