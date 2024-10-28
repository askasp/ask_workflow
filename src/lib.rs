use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    Extension, Router,
};

use db_trait::DB;
use tera::Tera;


pub mod activity;
pub mod db_trait;
pub mod ui;
pub mod worker;
pub mod workflow;
pub mod workflow_state;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
// Axum server setup
#[derive(Clone)]
struct AppState {
    worker: Arc<worker::Worker>,
}
pub async fn start_axum_server(worker:  Arc<worker::Worker>, port: u16) {
    let app_state = AppState { worker };
    let app = Router::new()
        .nest("/workflows", ui::controllers::workflow::routes())
        .layer(Extension(app_state));
    let addr: String = format!("0.0.0.0:{}", port).parse().unwrap();

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

