use crate::core::{HitboxWorld, StepExt};
use anyhow::{Error, anyhow};
use chrono::Utc;
use cucumber::gherkin::Step;
use cucumber::when;
use hurl::{
    runner::{VariableSet, request::eval_request},
    util::path::ContextDir,
};
use hurl_core::{error::DisplaySourceError, parser::parse_hurl_file, text::Format};

#[when(expr = "execute request")]
async fn execute_request(world: &mut HitboxWorld, step: &Step) -> Result<(), Error> {
    let hurl_request = step
        .docstring_content()
        .ok_or_else(|| anyhow!("request not provided"))?;

    let hurl_file = parse_hurl_file(&hurl_request).map_err(|err| {
        anyhow!(
            "hurl request parse error: {}",
            &err.message(&hurl_request.lines().collect::<Vec<_>>())
                .to_string(Format::Ansi)
        )
    })?;

    let variables = VariableSet::new();
    let parsed_request = &hurl_file
        .entries
        .first()
        .ok_or_else(|| anyhow!("request not found"))?
        .request;

    let request = eval_request(parsed_request, &variables, &ContextDir::default())
        .map_err(|err| anyhow!("hurl request error {:?}", err))?;

    world.execute_request(&request).await?;
    Ok(())
}

#[when(expr = "sleep {int}")]
async fn sleep(_world: &mut HitboxWorld, secs: u16) -> Result<(), Error> {
    tokio::time::sleep(tokio::time::Duration::from_secs(secs.into())).await;
    Ok(())
}

#[when(expr = "sleep {int}ms")]
async fn sleep_ms(_world: &mut HitboxWorld, millis: u64) -> Result<(), Error> {
    tokio::time::sleep(tokio::time::Duration::from_millis(millis)).await;
    Ok(())
}

#[when(expr = "wait for background tasks")]
async fn wait_for_background_tasks(world: &mut HitboxWorld) -> Result<(), Error> {
    if let Some(manager) = &world.offload_manager {
        // Wait up to 5 seconds for all tasks to complete
        let completed = manager
            .wait_all_timeout(std::time::Duration::from_secs(5))
            .await;
        if !completed {
            return Err(anyhow!("Background tasks did not complete within timeout"));
        }
    }
    Ok(())
}

#[when(expr = "debug cache")]
async fn debug_cache(world: &mut HitboxWorld) -> Result<(), Error> {
    use hitbox_backend::{Backend, CacheBackend};
    use hitbox_http::CacheableHttpResponse;

    eprintln!("=== DEBUG CACHE STATE ===");
    eprintln!("Cache entry count: {}", world.backend.cache.iter().count());
    for (key, value) in world.backend.cache.iter() {
        eprintln!("  Key: {:?}", key);
        eprintln!("  Value expire: {:?}", value.expire);
        eprintln!("  Value stale: {:?}", value.stale);

        // Test direct Moka cache.get
        let direct_get = world.backend.cache.get(key.as_ref()).await;
        eprintln!("  Direct Moka get: {:?}", direct_get.is_some());

        // Test Backend::read
        let backend_read = world.backend.read(key.as_ref()).await;
        eprintln!(
            "  Backend read: {:?}",
            backend_read.is_ok() && backend_read.as_ref().unwrap().is_some()
        );

        // Test full CacheBackend::get (with deserialization)
        let mut ctx = hitbox_core::CacheContext::default().boxed();
        let cache_get_result = world
            .backend
            .get::<CacheableHttpResponse<axum::body::Body>>(key.as_ref(), &mut ctx)
            .await;
        match &cache_get_result {
            Ok(Some(cached_value)) => {
                eprintln!("  CacheBackend::get: Ok(Some)");
                eprintln!("  CacheValue expire: {:?}", cached_value.expire);
                eprintln!("  CacheValue stale: {:?}", cached_value.stale);
                // Test cache_state (sync operation - just checks timestamps)
                let cache_state = cached_value.clone().cache_state();
                match cache_state {
                    hitbox::CacheState::Actual(_) => eprintln!("  cache_state: Actual (HIT)"),
                    hitbox::CacheState::Stale(_) => eprintln!("  cache_state: Stale"),
                    hitbox::CacheState::Expired(_) => eprintln!("  cache_state: Expired (MISS)"),
                }
            }
            Ok(None) => eprintln!("  CacheBackend::get: Ok(None)"),
            Err(e) => eprintln!("  CacheBackend::get: Err({:?})", e),
        }
    }
    eprintln!("Real time now: {:?}", Utc::now());
    eprintln!("=========================");
    Ok(())
}
