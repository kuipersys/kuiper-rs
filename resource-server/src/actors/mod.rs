pub mod models;

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::TryStreamExt;
use models::{ClientMessage, ServerMessage};
use tokio::sync::mpsc;

use crate::SubscriberMap;

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
        Err(actix_web::error::ErrorUnauthorized("Invalid bearer token format"))
    }
}

fn validate_token(token: &str) -> Result<String, actix_web::Error> {
    // Dummy example: validate and extract user_id
    if token == "supersecrettoken" {
        Ok("user-123".to_string()) // This could be a user ID or session ID
    } else {
        Err(actix_web::error::ErrorUnauthorized("Invalid token"))
    }
}

pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    subscribers: web::Data<SubscriberMap>,
) -> actix_web::Result<HttpResponse> {
    // let token = extract_bearer_token(&req)?;
    // let user_id = validate_token(&token)?;

    let (res, mut session, mut stream) = actix_ws::handle(&req, body)?;

    // Create sender/receiver for outbound messages to this client
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Create a unique client ID (use UUID or auth token in production)
    let client_id = uuid::Uuid::new_v4().to_string();

    // Register this client
    subscribers.insert(client_id.clone(), tx.clone());

    // Spawn the task to handle messages to/from this client
    actix_web::rt::spawn({
        let subscribers = subscribers.clone();

        async move {
            tx.send(ServerMessage::Hello {
                client_id: client_id.clone(),
                message: "Hello from server!".to_string(),
            })
            .unwrap_or_else(|_| {
                println!("Failed to send hello message to client {client_id}");
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
                                // parse and handle ClientMessage
                                println!("Client said: {txt}");
                            }
                            Message::Close(_) | Message::Ping(_) | Message::Pong(_) => break,
                            _ => {}
                        }
                    }
                }
            }

            // Unregister client on disconnect
            subscribers.remove(&client_id);
            let _ = session.close(None).await;
        }
    });

    Ok(res)
}