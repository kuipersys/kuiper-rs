//! HTTP integration tests for the resource server.
//!
//! These tests spin up the Actix-web app in-process using `actix_web::test`
//! (no real TCP port is bound) and exercise the full HTTP handler stack against
//! an in-memory store.

use actix_web::http::StatusCode;
use actix_web::{test, App};
use dashmap::DashMap;
use kuiper_runtime::KuiperRuntimeBuilder;
use kuiper_runtime_sdk::data::InMemoryStore;
use resource_server::{
    commands::observer::SetObserverCommand, configure_app, SubscriberMap, SubscriptionMap,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

// ─── helpers ────────────────────────────────────────────────────────────────

fn build_runtime() -> (
    Arc<kuiper_runtime::KuiperRuntime>,
    SubscriberMap,
    SubscriptionMap,
) {
    let store = InMemoryStore::new();
    let shared_store = Arc::new(RwLock::new(store));
    let subscribers: SubscriberMap = Arc::new(DashMap::new());
    let subscription_map: SubscriptionMap = Arc::new(DashMap::new());

    let mut builder = KuiperRuntimeBuilder::new(shared_store.clone());
    builder.register_handler(
        "set",
        Arc::new(SetObserverCommand::new(
            shared_store.clone(),
            subscribers.clone(),
            subscription_map.clone(),
        )),
    );

    let runtime = Arc::new(builder.build());
    (runtime, subscribers, subscription_map)
}

/// Convenience: initialise the Actix test service.
macro_rules! init_app {
    ($rt:expr, $subs:expr, $sub_map:expr) => {{
        let rt = $rt.clone();
        let subs = $subs.clone();
        let sub_map = $sub_map.clone();
        test::init_service(
            App::new().configure(move |cfg| {
                configure_app(cfg, rt.clone(), subs.clone(), sub_map.clone())
            }),
        )
        .await
    }};
}

// ─── version ────────────────────────────────────────────────────────────────

/// `GET /version` should return 200 with a `version` field.
#[actix_web::test]
async fn test_get_version_ok() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let req = test::TestRequest::get().uri("/version").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body.get("version").is_some(),
        "response must contain 'version'"
    );
}

// ─── PUT (create / update) ───────────────────────────────────────────────────

/// `PUT /api/{group}/{ns}/{kind}/{name}` with a valid body → 200.
#[actix_web::test]
async fn test_put_resource_created() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let body = json!({
        "apiVersion": "mygroup/v1",
        "kind": "Widget",
        "metadata": { "name": "my-widget", "namespace": "default" },
        "spec": { "color": "blue" }
    });

    let req = test::TestRequest::put()
        .uri("/api/mygroup/default/Widget/my-widget")
        .set_json(&body)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
}

/// `PUT /api/{group}/{ns}/{kind}` (no name) → 400.
#[actix_web::test]
async fn test_put_without_name_is_400() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let req = test::TestRequest::put()
        .uri("/api/mygroup/default/Widget")
        .set_json(&json!({ "apiVersion": "mygroup/v1", "kind": "Widget" }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── GET (single) ────────────────────────────────────────────────────────────

/// After a PUT, `GET /api/{group}/{ns}/{kind}/{name}` → 200 with the stored resource.
#[actix_web::test]
async fn test_get_resource_after_put() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let body = json!({
        "apiVersion": "mygroup/v1",
        "kind": "Widget",
        "metadata": { "name": "blue-widget", "namespace": "default" },
        "spec": { "color": "blue" }
    });

    // Create it
    let put = test::TestRequest::put()
        .uri("/api/mygroup/default/Widget/blue-widget")
        .set_json(&body)
        .to_request();
    test::call_service(&app, put).await;

    // Retrieve it
    let get = test::TestRequest::get()
        .uri("/api/mygroup/default/Widget/blue-widget")
        .to_request();
    let resp = test::call_service(&app, get).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let stored: Value = test::read_body_json(resp).await;
    assert_eq!(
        stored["metadata"]["name"].as_str(),
        Some("blue-widget"),
        "stored name must match"
    );
}

/// `GET` for a resource that was never created → 404.
#[actix_web::test]
async fn test_get_nonexistent_resource_is_404() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let req = test::TestRequest::get()
        .uri("/api/mygroup/default/Widget/ghost")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─── GET (list) ──────────────────────────────────────────────────────────────

/// `GET /api/{group}/{ns}/{kind}` (no name) → 200 with a JSON array.
#[actix_web::test]
async fn test_list_returns_array() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    // Seed two widgets
    for name in &["w1", "w2"] {
        let put = test::TestRequest::put()
            .uri(&format!("/api/mygroup/default/Widget/{name}"))
            .set_json(&json!({
                "apiVersion": "mygroup/v1",
                "kind": "Widget",
                "metadata": { "name": name, "namespace": "default" },
                "spec": {}
            }))
            .to_request();
        test::call_service(&app, put).await;
    }

    // List
    let req = test::TestRequest::get()
        .uri("/api/mygroup/default/Widget")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let items: Value = test::read_body_json(resp).await;
    assert!(items.is_array(), "list response must be a JSON array");
    assert!(
        items.as_array().unwrap().len() >= 2,
        "should contain at least the two seeded widgets"
    );
}

/// Listing an empty namespace/kind → 200 with an empty array.
#[actix_web::test]
async fn test_list_empty_returns_empty_array() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let req = test::TestRequest::get()
        .uri("/api/mygroup/default/NoSuchKind")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let items: Value = test::read_body_json(resp).await;
    assert_eq!(items, json!([]), "empty list must return []");
}

// ─── DELETE ──────────────────────────────────────────────────────────────────

/// After DELETE the resource is soft-deleted and a subsequent GET returns 404.
#[actix_web::test]
async fn test_delete_then_get_is_404() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    // Create
    let put = test::TestRequest::put()
        .uri("/api/mygroup/default/Widget/doomed")
        .set_json(&json!({
            "apiVersion": "mygroup/v1",
            "kind": "Widget",
            "metadata": { "name": "doomed", "namespace": "default" },
            "spec": {}
        }))
        .to_request();
    test::call_service(&app, put).await;

    // Delete
    let del = test::TestRequest::delete()
        .uri("/api/mygroup/default/Widget/doomed")
        .to_request();
    let del_resp = test::call_service(&app, del).await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    // Should be gone now
    let get = test::TestRequest::get()
        .uri("/api/mygroup/default/Widget/doomed")
        .to_request();
    let get_resp = test::call_service(&app, get).await;
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
}

/// `DELETE /api/{group}/{ns}/{kind}` (no name) → 400.
#[actix_web::test]
async fn test_delete_without_name_is_400() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let req = test::TestRequest::delete()
        .uri("/api/mygroup/default/Widget")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── routing ─────────────────────────────────────────────────────────────────

/// Paths with fewer than 3 segments → 400.
#[actix_web::test]
async fn test_short_path_is_400() {
    let (rt, subs, sub_map) = build_runtime();
    let app = init_app!(rt, subs, sub_map);

    let req = test::TestRequest::get().uri("/api/onlyone").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
