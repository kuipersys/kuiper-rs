pub mod models;

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::TryStreamExt;
use kuiper_runtime::command::CommandContext;
use kuiper_runtime::KuiperRuntime;
use models::{ClientMessage, ServerMessage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{SubscriberMap, SubscriptionMap};

fn extract_bearer_token(req: &HttpRequest) -> Result<String, actix_web::Error> {
    let header = req
        .headers()
        .get("Authorization")
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Missing Authorization header"))?;

    let header_str = header
        .to_str()
        .map_err(|_| actix_web::error::ErrorUnauthorized("Invalid header format"))?;

    if let Some(token) = header_str.strip_prefix("Bearer ") {
        Ok(token.to_string())
    } else {
        Err(actix_web::error::ErrorUnauthorized(
            "Invalid bearer token format",
        ))
    }
}

fn validate_token(token: &str) -> Result<String, actix_web::Error> {
    // Dummy example: validate and extract user_id
    if token == "supersecrettoken" {
        Ok("user-123".to_string())
    } else {
        Err(actix_web::error::ErrorUnauthorized("Invalid token"))
    }
}

pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    subscribers: web::Data<SubscriberMap>,
    subscription_map: web::Data<SubscriptionMap>,
    rt: web::Data<Arc<KuiperRuntime>>,
) -> actix_web::Result<HttpResponse> {
    // let token = extract_bearer_token(&req)?;
    // let user_id = validate_token(&token)?;

    let (res, mut session, mut stream) = actix_ws::handle(&req, body)?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let client_id = uuid::Uuid::new_v4().to_string();

    subscribers.insert(client_id.clone(), tx.clone());
    subscription_map.insert(client_id.clone(), Vec::new());

    actix_web::rt::spawn({
        let subscribers = subscribers.clone();
        let subscription_map = subscription_map.clone();
        let rt = rt.clone();

        async move {
            tx.send(ServerMessage::Hello {
                client_id: client_id.clone(),
                message: "Hello from server!".to_string(),
            })
            .unwrap_or_else(|_| {
                tracing::warn!("Failed to send hello message to client {client_id}");
            });

            loop {
                tokio::select! {
                    // Outbound messages
                    Some(msg) = rx.recv() => {
                        let text = serde_json::to_string(&msg).unwrap();
                        if session.text(text).await.is_err() {
                            break;
                        }
                    }
                    // Inbound messages
                    Ok(Some(msg)) = stream.try_next() => {
                        match msg {
                            Message::Text(txt) => {
                                match serde_json::from_str::<ClientMessage>(&txt) {
                                    Ok(ClientMessage::Subscribe { resource }) => {
                                        if let Some(mut subs) = subscription_map.get_mut(&client_id) {
                                            if !subs.contains(&resource) {
                                                subs.push(resource.clone());
                                            }
                                        }
                                        let _ = tx.send(ServerMessage::Subscribed { resource });
                                    }
                                    Ok(ClientMessage::Rpc { method, payload }) => {
                                        let mut ctx = CommandContext {
                                            command_name: method,
                                            parameters: HashMap::new(),
                                            metadata: HashMap::new(),
                                            activity_id: uuid::Uuid::new_v4(),
                                            caller_id: None,
                                            cancellation_token: CancellationToken::new(),
                                            is_internal: false,
                                        };
                                        // Flatten JSON object payload into individual parameters.
                                        if let Some(obj) = payload.as_object() {
                                            for (k, v) in obj {
                                                ctx.parameters.insert(k.clone(), v.clone());
                                            }
                                        }
                                        let response = match rt.execute(&mut ctx).await {
                                            Ok(Some(val)) => ServerMessage::RpcResult { value: val },
                                            Ok(None) => ServerMessage::RpcResult {
                                                value: serde_json::Value::Null,
                                            },
                                            Err(e) => ServerMessage::Error {
                                                message: e.to_string(),
                                            },
                                        };
                                        let _ = tx.send(response);
                                    }
                                    Err(e) => {
                                        let _ = tx.send(ServerMessage::Error {
                                            message: format!("Invalid message: {}", e),
                                        });
                                    }
                                }
                            }
                            Message::Close(_) => break,
                            // actix-ws handles ping/pong automatically; ignore here.
                            _ => {}
                        }
                    }
                }
            }

            // Unregister client on disconnect
            subscribers.remove(&client_id);
            subscription_map.remove(&client_id);
            let _ = session.close(None).await;
        }
    });

    Ok(res)
}
