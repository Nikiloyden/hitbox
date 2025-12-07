use crate::app::app;
use crate::handler_state::HandlerState;
use crate::mock_backend::MockBackend;
use hitbox::concurrency::BroadcastConcurrencyManager;
use hitbox::offload::OffloadManager;
use hitbox_configuration::Endpoint;
use hitbox_tower::Cache;
use std::fmt::Debug;
use std::str::FromStr;

use anyhow::Error;
use axum_test::{TestResponse, TestServer};
use cucumber::World;
use cucumber::gherkin::Step;
use hurl::http::{Body, RequestSpec};

#[derive(Debug, Default)]
pub struct State {
    pub response: Option<TestResponse>,
    pub responses: Vec<TestResponse>,
}

#[derive(Debug, World)]
pub struct HitboxWorld {
    pub config: Endpoint<axum::body::Body, axum::body::Body>,
    pub state: State,
    pub backend: MockBackend,
    #[world(default)]
    pub offload_manager: Option<OffloadManager>,
    pub handler_state: HandlerState,
}

impl Default for HitboxWorld {
    fn default() -> Self {
        Self {
            config: Default::default(),
            state: Default::default(),
            backend: MockBackend::new(),
            offload_manager: None,
            handler_state: HandlerState::new(),
        }
    }
}

impl HitboxWorld {
    pub async fn execute_request(&mut self, request_spec: &RequestSpec) -> Result<(), Error> {
        let concurrency_manager: BroadcastConcurrencyManager<_> =
            BroadcastConcurrencyManager::new();

        let server = if let Some(manager) = &self.offload_manager {
            let cache = Cache::builder()
                .backend(self.backend.clone())
                .config(self.config.clone())
                .concurrency_manager(concurrency_manager)
                .offload(manager.clone())
                .build();
            let router = app(self.handler_state.clone()).layer(cache);
            TestServer::new(router)?
        } else {
            let cache = Cache::builder()
                .backend(self.backend.clone())
                .config(self.config.clone())
                .concurrency_manager(concurrency_manager)
                .build();
            let router = app(self.handler_state.clone()).layer(cache);
            TestServer::new(router)?
        };
        let path = request_spec.url.path();
        let mut request = server.method(
            http::Method::from_str(request_spec.method.0.to_string().as_str())?,
            path.as_str(),
        );
        for header in &request_spec.headers {
            request = request.add_header(&header.name, &header.value);
        }
        // Add query params from URL
        for param in request_spec.url.query_params() {
            request = request.add_query_param(&param.name, &param.value);
        }
        // Add query params from [Query] section
        for param in &request_spec.querystring {
            request = request.add_query_param(&param.name, &param.value);
        }

        // Set request body based on content type
        // Use .text() and .bytes() methods which don't modify headers,
        // unlike .json() which automatically sets Content-Type: application/json
        let request = match &request_spec.body {
            Body::Text(body) if !body.is_empty() => request.text(body),
            Body::File(body, _name) if !body.is_empty() => request.bytes(body.clone().into()),
            Body::Binary(bin) if !bin.is_empty() => request.bytes(bin.clone().into()),
            _ => request, // No body
        };

        let response = request.await;
        self.state.response = Some(response);

        Ok(())
    }
}

pub trait StepExt {
    fn docstring_content(&self) -> Option<String>;
}

impl StepExt for Step {
    fn docstring_content(&self) -> Option<String> {
        self.docstring()
            .map(|docstring| docstring.lines().skip(1).collect::<Vec<_>>().join("\n"))
    }
}
