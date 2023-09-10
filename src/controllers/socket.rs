use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use log::{debug, error, warn};
use serde_json::Value;
use tokio::sync::Mutex;
use warp::ws::{Message, WebSocket};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use crate::controllers::channel::ChannelHandler;
use crate::SERVER_LIST;

lazy_static! {
    static ref API: webrtc::api::API = APIBuilder::new().build();
}

pub async fn ws_connected(ws: WebSocket) {
    let mut connection: Option<RTCPeerConnection> = None;

    let (ws_tx, mut ws_rx) = ws.split();
    let ws_tx = Arc::new(Mutex::new(ws_tx));

    while let Some(Ok(message)) = ws_rx.next().await {
        let message: Value = match serde_json::from_slice(message.as_bytes()) {
            Ok(message) => message,
            Err(err) => {
                debug!("Failed to deserialize message. {}", err);
                break;
            }
        };
        if let Some(Ok(offer)) = message.get("offer").map(|v| {
            serde_json::from_value::<RTCSessionDescription>(v.clone()).map_err(|e| {
                debug!("Failed to deserialize offer. {}", e);
                e
            })
        }) {
            if let Some(connection) = connection.take() {
                if let Err(err) = connection.close().await {
                    debug!("Failed to close connection. {}", err);
                    break;
                }
            }

            let servers: Vec<String>;
            {
                let server_list = SERVER_LIST.lock().unwrap();
                servers = server_list.clone();
            }

            let new_connection = match API
                .new_peer_connection(RTCConfiguration {
                    ice_servers: servers
                        .iter()
                        .map(|e| RTCIceServer {
                            urls: vec![e.to_owned()],
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                })
                .await
            {
                Ok(connection) => connection,
                Err(err) => {
                    debug!("Failed to create peer connection. {}", err);
                    break;
                }
            };

            {
                let ws_tx = ws_tx.clone();
                new_connection.on_ice_candidate(Box::new(move |candidate| {
                    let ws_tx = ws_tx.clone();
                    Box::pin(async move {
                        let Some(candidate) = candidate else {
                            return;
                        };
                        debug!("ICE candidate: {}:{}", candidate.address, candidate.port);
                        let msg = serde_json::to_string(&serde_json::json!({
                            "ice": candidate,
                        }))
                        .unwrap();
                        match ws_tx.lock().await.send(Message::text(msg)).await {
                            Ok(_) => {}
                            Err(err) => {
                                warn!("Failed to send ICE candidate. {}", err);
                            }
                        }
                    })
                }));
            }

            new_connection.on_data_channel(Box::new(move |channel| {
                debug!("Data channel opened");
                let mut handler = ChannelHandler::new(channel);
                Box::pin(async move {
                    tokio::spawn(async move {
                        if let Err(err) = handler.run().await {
                            error!("channel handler: {}", err);
                        }
                    });
                })
            }));

            if let Err(err) = new_connection.set_remote_description(offer).await {
                debug!("Failed to set remote description. {}", err);
                break;
            }
            let answer = match new_connection.create_answer(None).await {
                Ok(answer) => answer,
                Err(err) => {
                    debug!("Failed to create answer. {}", err);
                    break;
                }
            };
            if let Err(err) = new_connection.set_local_description(answer).await {
                debug!("Failed to set local description. {}", err);
                break;
            }
            let Ok(msg) = serde_json::to_string(&serde_json::json!({
                "answer": new_connection.local_description().await,
            })) else {
                debug!("Failed to serialize answer");
                break;
            };

            if let Err(err) = ws_tx.lock().await.send(Message::text(msg)).await {
                debug!("Failed to send answer. {}", err);
                break;
            }

            connection = Some(new_connection);
        }

        if let Some(Ok(ice)) = message.get("ice").map(|v| {
            serde_json::from_value::<RTCIceCandidateInit>(v.clone()).map_err(|e| {
                debug!("Failed to deserialize ice candidate. {}", e);
                e
            })
        }) {
            if let Some(connection) = &mut connection {
                if let Err(err) = connection.add_ice_candidate(ice).await {
                    debug!("Failed to add ICE candidate. {}", err);
                    break;
                }
            }
        }
    }

    if let Some(connection) = connection {
        if let Err(err) = connection.close().await {
            debug!("Failed to close connection. {}", err);
        }
    }
}
