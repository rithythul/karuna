use axum::http::StatusCode;
use axum_test::TestServer;

#[tokio::test]
async fn test_health_endpoint() {
    let app = axum::Router::new()
        .route("/health", axum::routing::get(|| async {
            axum::Json(serde_json::json!({"status": "ok"}))
        }));

    let server = TestServer::new(app);
    let response: axum_test::TestResponse = server.get("/health").await;
    response.assert_status(StatusCode::OK);
    response.assert_json(&serde_json::json!({"status": "ok"}));
}
