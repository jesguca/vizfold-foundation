mod config;
mod db;
mod entities;
mod migrations;
mod repositories;
mod services;

use axum::{Json, Router, routing::get};
use serde_json::json;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let db = db::connect_and_migrate()
        .await
        .expect("failed to initialize database");

    let _ = services::model_backends::list_model_backends(&db)
        .await
        .expect("failed to query model backends");

    let app = Router::new().route("/health", get(health));
    let listener = TcpListener::bind("127.0.0.1:3001")
        .await
        .expect("failed to bind TCP listener");

    println!("API listening on http://127.0.0.1:3001");
    println!("Database connected using {}", config::database_url());

    axum::serve(listener, app)
        .await
        .expect("server error");
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
