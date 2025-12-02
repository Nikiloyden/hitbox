use crate::fsm::world::FsmWorld;
use anyhow::{Error, anyhow};
use cucumber::then;

// =============================================================================
// Upstream Call Assertions
// =============================================================================

#[then(expr = "upstream should be called {int} time")]
fn upstream_called_once(world: &mut FsmWorld, expected: usize) -> Result<(), Error> {
    let actual = world.results.upstream_call_count();
    if actual != expected {
        return Err(anyhow!(
            "Expected upstream to be called {} time(s), but was called {} time(s)",
            expected,
            actual
        ));
    }
    Ok(())
}

#[then(expr = "upstream should be called {int} times")]
fn upstream_called_times(world: &mut FsmWorld, expected: usize) -> Result<(), Error> {
    let actual = world.results.upstream_call_count();
    if actual != expected {
        return Err(anyhow!(
            "Expected upstream to be called {} time(s), but was called {} time(s)",
            expected,
            actual
        ));
    }
    Ok(())
}

#[then(expr = "upstream should not be called")]
fn upstream_not_called(world: &mut FsmWorld) -> Result<(), Error> {
    let actual = world.results.upstream_call_count();
    if actual != 0 {
        return Err(anyhow!(
            "Expected upstream to not be called, but was called {} time(s)",
            actual
        ));
    }
    Ok(())
}

#[then(expr = "upstream should be called at most {int} times")]
fn upstream_called_at_most(world: &mut FsmWorld, max_expected: usize) -> Result<(), Error> {
    let actual = world.results.upstream_call_count();
    if actual > max_expected {
        return Err(anyhow!(
            "Expected upstream to be called at most {} time(s), but was called {} time(s)",
            max_expected,
            actual
        ));
    }
    Ok(())
}

// =============================================================================
// Response Assertions
// =============================================================================

#[then(expr = "all responses should equal {int}")]
fn all_responses_equal(world: &mut FsmWorld, expected: u32) -> Result<(), Error> {
    if !world.results.all_responses_eq(expected) {
        let actual_values: Vec<_> = world.results.responses.iter().map(|(r, _)| r.0).collect();
        return Err(anyhow!(
            "Expected all responses to equal {}, but got {:?}",
            expected,
            actual_values
        ));
    }
    Ok(())
}

// =============================================================================
// Cache State Assertions
// =============================================================================

#[then(expr = "cache should be empty")]
async fn cache_is_empty(world: &mut FsmWorld) -> Result<(), Error> {
    if !world.cache_is_empty().await {
        return Err(anyhow!(
            "Expected cache to be empty, but it contains a value"
        ));
    }
    Ok(())
}

#[then(expr = "cache should contain value {int}")]
async fn cache_contains(world: &mut FsmWorld, expected: u32) -> Result<(), Error> {
    if !world.cache_contains_value(expected).await {
        return Err(anyhow!(
            "Expected cache to contain value {}, but it doesn't",
            expected
        ));
    }
    Ok(())
}

// =============================================================================
// Cache Status Assertions
// =============================================================================

#[then(expr = "cache status should be {string}")]
fn cache_status(world: &mut FsmWorld, expected_status: String) -> Result<(), Error> {
    // Check the cache status from the first response context
    if let Some((_, context)) = world.results.responses.first() {
        let actual_status = format!("{:?}", context.status);
        // Normalize comparison (Hit vs CacheHit, etc.)
        let matches = match expected_status.as_str() {
            "Hit" => actual_status.contains("Hit"),
            "Miss" => actual_status.contains("Miss"),
            _ => actual_status == expected_status,
        };
        if !matches {
            return Err(anyhow!(
                "Expected cache status '{}', but got '{}'",
                expected_status,
                actual_status
            ));
        }
    } else {
        return Err(anyhow!("No responses available to check cache status"));
    }
    Ok(())
}

// =============================================================================
// FSM State Assertions (only with fsm-trace feature)
// =============================================================================

#[cfg(feature = "fsm-trace")]
#[then("FSM states should be:")]
fn fsm_states_should_be(world: &mut FsmWorld, step: &cucumber::gherkin::Step) -> Result<(), Error> {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| anyhow!("Expected a table of FSM states"))?;

    let expected: Vec<&str> = table
        .rows
        .iter()
        .filter_map(|row| row.first().map(|s| s.as_str()))
        .collect();

    if let Some(actual_states) = world.results.fsm_states() {
        let actual: Vec<&str> = actual_states.iter().map(|s| s.as_str()).collect();
        if actual != expected {
            return Err(anyhow!(
                "Expected FSM states:\n  {:?}\nbut got:\n  {:?}",
                expected,
                actual
            ));
        }
    } else {
        return Err(anyhow!("No responses available to check FSM states"));
    }
    Ok(())
}

#[cfg(feature = "fsm-trace")]
#[then(expr = "FSM states should contain {string}")]
fn fsm_states_should_contain(world: &mut FsmWorld, expected_state: String) -> Result<(), Error> {
    if let Some(actual_states) = world.results.fsm_states() {
        if !actual_states.iter().any(|s| s == &expected_state) {
            return Err(anyhow!(
                "Expected FSM states to contain '{}', but got {:?}",
                expected_state,
                actual_states
            ));
        }
    } else {
        return Err(anyhow!("No responses available to check FSM states"));
    }
    Ok(())
}

/// Multi-column FSM state verification for concurrent requests.
/// Each column represents a different request's expected FSM path.
/// Empty cells are skipped (shorter paths for waiters).
#[cfg(feature = "fsm-trace")]
#[then("FSM states for each request should be:")]
fn fsm_states_for_each_request(
    world: &mut FsmWorld,
    step: &cucumber::gherkin::Step,
) -> Result<(), Error> {
    let table = step
        .table
        .as_ref()
        .ok_or_else(|| anyhow!("Expected a table of FSM states"))?;

    // Get all actual FSM states
    let all_actual = world.results.all_fsm_states();
    if all_actual.is_empty() {
        return Err(anyhow!("No responses available to check FSM states"));
    }

    // Determine number of columns (requests) from the first row
    let num_columns = table.rows.first().map(|row| row.len()).unwrap_or(0);

    if num_columns == 0 {
        return Err(anyhow!("Table has no columns"));
    }

    if all_actual.len() != num_columns {
        return Err(anyhow!(
            "Expected {} requests but got {} responses",
            num_columns,
            all_actual.len()
        ));
    }

    // Build expected states per column (request)
    // Skip the first row if it looks like a header (contains "Request")
    let rows_to_process: Vec<_> = table.rows.iter().collect();
    let skip_header = rows_to_process
        .first()
        .is_some_and(|row| row.first().is_some_and(|cell| cell.contains("Request")));

    let mut expected_per_request: Vec<Vec<&str>> = vec![Vec::new(); num_columns];
    for row in rows_to_process.iter().skip(if skip_header { 1 } else { 0 }) {
        for (col_idx, cell) in row.iter().enumerate() {
            let trimmed = cell.trim();
            if !trimmed.is_empty() {
                expected_per_request[col_idx].push(trimmed);
            }
        }
    }

    // Compare each request's actual states with expected
    for (request_idx, expected) in expected_per_request.iter().enumerate() {
        let actual: Vec<&str> = all_actual[request_idx].iter().map(|s| s.as_str()).collect();
        if actual != *expected {
            return Err(anyhow!(
                "Request {} FSM states mismatch:\n  Expected: {:?}\n  Actual:   {:?}",
                request_idx + 1,
                expected,
                actual
            ));
        }
    }

    Ok(())
}

// =============================================================================
// Composition L1/L2 Assertions
// =============================================================================

#[then(expr = "L{int} should be empty")]
async fn layer_is_empty(world: &mut FsmWorld, layer: u8) -> Result<(), Error> {
    let is_empty = match layer {
        1 => world.l1_is_empty().await,
        2 => world.l2_is_empty().await,
        _ => return Err(anyhow!("Unknown layer: L{}", layer)),
    };
    if !is_empty {
        return Err(anyhow!(
            "Expected L{} to be empty, but it contains a value",
            layer
        ));
    }
    Ok(())
}

#[then(expr = "L{int} should contain value {int}")]
async fn layer_contains_value(world: &mut FsmWorld, layer: u8, expected: u32) -> Result<(), Error> {
    let contains = match layer {
        1 => world.l1_contains_value(expected).await,
        2 => world.l2_contains_value(expected).await,
        _ => return Err(anyhow!("Unknown layer: L{}", layer)),
    };
    if !contains {
        return Err(anyhow!(
            "Expected L{} to contain value {}, but it doesn't",
            layer,
            expected
        ));
    }
    Ok(())
}
