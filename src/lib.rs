use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    Extension, Router,
};

use db_trait::DB;
use tera::Tera;
use ui::controllers;

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
    db: Arc<dyn DB>,
}
pub async fn start_axum_server(db: Arc<dyn DB>, port: u16) {
    let app_state = AppState { db };

    let app = Router::new()
        .nest("/workflows", ui::controllers::workflow::routes())
        .layer(Extension(app_state));
    let addr: String = format!("0.0.0.0:{}", port).parse().unwrap();

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

pub fn common_context() -> tera::Context {
    let mut context = tera::Context::new();
    context.insert("title", "axum-tera");
    context
}

pub fn tera_include() -> Tera {
    let tera = Tera::new("src/ui/views/**/*").unwrap();
    tera
}
