use std::time::Duration;

use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::get,
};

use hitbox::policy::PolicyConfig;
use hitbox::predicate::PredicateExt;
use hitbox_configuration::Endpoint;
use hitbox_http::{
    extractors::{
        Method as MethodExtractor, path::PathExtractor,
        query::QueryExtractor as QueryExtractorTrait,
    },
    predicates::{
        body::{BodyPredicate, JqExpression, JqOperation, Operation as BodyOperation},
        header::{Header as RequestHeader, Operation as HeaderOperation},
        // Uncomment for method/path predicates (see example in list_config):
        // request::{Method as RequestMethod, PathPredicate},
        response::StatusCode as ResponseStatusCode,
    },
};
use hitbox_tower::Cache;
// Uncomment for method predicate: use http::Method;
use serde::{Deserialize, Serialize};

// =============================================================================
// Domain Types
// =============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: u32,
    pub title: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub status: Option<TaskStatus>,
}

fn default_page() -> u32 {
    1
}
fn default_limit() -> u32 {
    10
}

#[derive(Debug, Serialize)]
pub struct TaskList {
    pub tasks: Vec<Task>,
    pub total: u32,
    pub page: u32,
    pub limit: u32,
}

// =============================================================================
// Mock Data
// =============================================================================

fn get_all_tasks() -> Vec<Task> {
    vec![
        Task {
            id: 1,
            title: "Set up project structure".into(),
            status: TaskStatus::Completed,
        },
        Task {
            id: 2,
            title: "Implement authentication".into(),
            status: TaskStatus::InProgress,
        },
        Task {
            id: 3,
            title: "Write unit tests".into(),
            status: TaskStatus::Pending,
        },
        Task {
            id: 4,
            title: "Add pagination".into(),
            status: TaskStatus::Pending,
        },
        Task {
            id: 5,
            title: "Set up CI/CD".into(),
            status: TaskStatus::Pending,
        },
        Task {
            id: 6,
            title: "Add caching layer".into(),
            status: TaskStatus::InProgress,
        },
        Task {
            id: 7,
            title: "Write API documentation".into(),
            status: TaskStatus::Pending,
        },
        Task {
            id: 8,
            title: "Performance optimization".into(),
            status: TaskStatus::Pending,
        },
    ]
}

// =============================================================================
// Handlers
// =============================================================================

async fn list_tasks(Query(params): Query<ListParams>) -> Json<TaskList> {
    tracing::info!(
        "Fetching task list: page={}, limit={}",
        params.page,
        params.limit
    );

    let all_tasks = get_all_tasks();

    // Filter by status if provided
    let filtered: Vec<_> = match params.status {
        Some(status) => all_tasks
            .into_iter()
            .filter(|t| t.status == status)
            .collect(),
        None => all_tasks,
    };

    let total = filtered.len() as u32;

    // Paginate
    let start = ((params.page - 1) * params.limit) as usize;
    let tasks: Vec<_> = filtered
        .into_iter()
        .skip(start)
        .take(params.limit as usize)
        .collect();

    Json(TaskList {
        tasks,
        total,
        page: params.page,
        limit: params.limit,
    })
}

async fn get_task(Path(task_id): Path<u32>) -> Result<Json<Task>, http::StatusCode> {
    tracing::info!("Fetching task details: id={}", task_id);

    get_all_tasks()
        .into_iter()
        .find(|t| t.id == task_id)
        .map(Json)
        .ok_or(http::StatusCode::NOT_FOUND)
}

async fn health() -> &'static str {
    "OK"
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::fmt()
        .pretty()
        .with_env_filter("debug,hitbox=trace")
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let memory_backend = hitbox_moka::MokaBackend::builder(1024 * 1024).build();

    // Cache config for task list endpoint
    // Cache key includes: pagination params
    //
    // Note: Method and path predicates are not needed here because the cache
    // layer is applied per-route in axum. The router already ensures only
    // GET /tasks requests reach this cache layer. Example of how to use them:
    //
    //     RequestMethod::new(Method::GET)
    //         .unwrap()
    //         .path("/tasks".to_string())
    //         .and(HeaderPredicate::new(...).not())
    //
    let list_config = Endpoint::builder()
        .request_predicate(
            // Skip cache if Cache-Control: no-cache (RFC 9111)
            RequestHeader::new(HeaderOperation::Contains(
                http::header::CACHE_CONTROL,
                "no-cache".to_string(),
            ))
            .not(),
        )
        .response_predicate(
            ResponseStatusCode::new(http::StatusCode::OK)
                // Skip cache if tasks list is empty
                .body(BodyOperation::Jq {
                    filter: JqExpression::compile(".tasks | length > 0").unwrap(),
                    operation: JqOperation::Eq(serde_json::Value::Bool(true)),
                }),
        )
        .extractor(
            MethodExtractor::new()
                .query("page".to_string())
                .query("limit".to_string())
                .query("status".to_string()),
        )
        .policy(PolicyConfig::builder().ttl(Duration::from_secs(60)).build())
        .build();

    // Cache config for task details endpoint
    // Cache key includes: task_id from path
    let details_config = Endpoint::builder()
        .request_predicate(
            // Skip cache if Cache-Control: no-cache (RFC 9111)
            RequestHeader::new(HeaderOperation::Contains(
                http::header::CACHE_CONTROL,
                "no-cache".to_string(),
            ))
            .not(),
        )
        .response_predicate(ResponseStatusCode::new(http::StatusCode::OK))
        .extractor(MethodExtractor::new().path("/tasks/{task_id}"))
        .policy(
            PolicyConfig::builder()
                .ttl(Duration::from_secs(300))
                .build(),
        )
        .build();

    let list_cache = Cache::builder()
        .backend(memory_backend.clone())
        .config(list_config)
        .build();

    let details_cache = Cache::builder()
        .backend(memory_backend)
        .config(details_config)
        .build();

    let app = Router::new()
        .route("/tasks", get(list_tasks).layer(list_cache))
        .route("/tasks/{task_id}", get(get_task).layer(details_cache))
        .route("/health", get(health));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    println!();
    println!("The X-Cache-Status header shows the cache status (hit/miss/stale).");
    println!();
    println!("Try these endpoints:");
    println!("  curl -v http://localhost:3000/tasks");
    println!("  curl -v http://localhost:3000/tasks?page=1&limit=3");
    println!("  curl -v http://localhost:3000/tasks?status=pending");
    println!("  curl -v http://localhost:3000/tasks/1");
    println!("  curl -v http://localhost:3000/health");
    println!();
    println!("Bypass cache with Cache-Control header (RFC 9111):");
    println!("  curl -v -H 'Cache-Control: no-cache' http://localhost:3000/tasks");
    println!("  curl -v -H 'Cache-Control: no-cache' http://localhost:3000/tasks/1");
    println!();
    axum::serve(listener, app).await.unwrap();
}
