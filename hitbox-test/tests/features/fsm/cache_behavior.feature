Feature: Cache FSM Behavior

  Background:
    Given upstream response delay is 100ms
    And request delay between concurrent requests is 10ms

  # =============================================================================
  # Cache Disabled (Test Case 1)
  # =============================================================================

  @fsm @disabled
  Scenario: Cache disabled - direct upstream call
    Given cache policy is "Disabled"
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And FSM states should be:
      | PollUpstream   |
      | UpstreamPolled |
      | Response       |

  # =============================================================================
  # Request Non-Cacheable (Test Case 4)
  # =============================================================================

  @fsm @request-noncacheable
  Scenario: Non-cacheable request bypasses cache
    Given cache policy is "Enabled"
    And request is non-cacheable
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should be empty
    And FSM states should be:
      | CheckRequestCachePolicy |
      | PollUpstream            |
      | UpstreamPolled          |
      | Response                |

  # =============================================================================
  # Cache Miss - No Dogpile Prevention (Test Cases 8, 9)
  # =============================================================================

  @fsm @miss @no-dogpile
  Scenario: Cache miss without dogpile prevention - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache is empty
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should be empty
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckConcurrency         |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | Response                 |

  @fsm @miss @no-dogpile
  Scenario: Cache miss without dogpile prevention - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache is empty
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should contain value 100
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckConcurrency         |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | UpdateCache              |
      | Response                 |

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Proceed with Permit (Test Cases 18, 19)
  # =============================================================================

  @fsm @miss @dogpile @proceed
  Scenario: Cache miss with permit - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache is empty
    And concurrency limit is 1
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should be empty
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckConcurrency         |
      | ConcurrentPollUpstream   |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | Response                 |

  @fsm @miss @dogpile @proceed
  Scenario: Cache miss with permit - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache is empty
    And concurrency limit is 1
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should contain value 100
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckConcurrency         |
      | ConcurrentPollUpstream   |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | UpdateCache              |
      | Response                 |

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Await Broadcast OK (Test Case 16)
  # =============================================================================

  @fsm @miss @dogpile @await @broadcast-ok
  Scenario: Concurrent requests - waiter receives broadcast successfully
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache is empty
    And concurrency limit is 1
    When 3 requests are made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should contain value 100
    And FSM states for each request should be:
      | Request 1                | Request 2                | Request 3                |
      | CheckRequestCachePolicy  | CheckRequestCachePolicy  | CheckRequestCachePolicy  |
      | PollCache                | PollCache                | PollCache                |
      | CheckConcurrency         | CheckConcurrency         | CheckConcurrency         |
      | ConcurrentPollUpstream   | ConcurrentPollUpstream   | ConcurrentPollUpstream   |
      | PollUpstream             | AwaitResponse            | AwaitResponse            |
      | UpstreamPolled           | Response                 | Response                 |
      | CheckResponseCachePolicy |                          |                          |
      | UpdateCache              |                          |                          |
      | Response                 |                          |                          |

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Non-cacheable Response (natural channel close)
  # =============================================================================

  @fsm @miss @dogpile @await @response-noncacheable
  Scenario: Concurrent requests with non-cacheable response - all hit upstream
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache is empty
    And concurrency limit is 1
    When 3 requests are made with value 100
    Then upstream should be called 3 times
    And all responses should equal 100
    And cache should be empty
    And FSM states for each request should be:
      | Request 1                | Request 2                | Request 3                |
      | CheckRequestCachePolicy  | CheckRequestCachePolicy  | CheckRequestCachePolicy  |
      | PollCache                | PollCache                | PollCache                |
      | CheckConcurrency         | CheckConcurrency         | CheckConcurrency         |
      | ConcurrentPollUpstream   | ConcurrentPollUpstream   | ConcurrentPollUpstream   |
      | PollUpstream             | AwaitResponse            | AwaitResponse            |
      | UpstreamPolled           | PollUpstream             | PollUpstream             |
      | CheckResponseCachePolicy | UpstreamPolled           | UpstreamPolled           |
      | Response                 | CheckResponseCachePolicy | CheckResponseCachePolicy |
      |                          | Response                 | Response                 |

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Await Broadcast Closed (Test Cases 11, 12)
  # Note: These scenarios require special setup to force channel closed
  # =============================================================================

  @fsm @miss @dogpile @await @broadcast-closed @allow.failed
  Scenario: Waiter falls back to upstream when channel closed - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache is empty
    And concurrency limit is 1
    And broadcast channel will be closed
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  @fsm @miss @dogpile @await @broadcast-closed @allow.failed
  Scenario: Waiter falls back to upstream when channel closed - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache is empty
    And concurrency limit is 1
    And broadcast channel will be closed
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Await Broadcast Lagged (Test Cases 14, 15)
  # Note: These scenarios require special setup to force channel lagged
  # =============================================================================

  @fsm @miss @dogpile @await @broadcast-lagged @allow.failed
  Scenario: Waiter falls back to upstream when channel lagged - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache is empty
    And concurrency limit is 1
    And broadcast channel will lag
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  @fsm @miss @dogpile @await @broadcast-lagged @allow.failed
  Scenario: Waiter falls back to upstream when channel lagged - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache is empty
    And concurrency limit is 1
    And broadcast channel will lag
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  # =============================================================================
  # Cache Hit - Actual/Fresh (Test Case 34)
  # =============================================================================

  @fsm @hit @actual
  Scenario: Fresh cache hit returns cached response immediately
    Given cache policy is "Enabled"
    And request is cacheable
    And cache contains fresh value 200
    When 1 request is made with value 100
    Then upstream should be called 0 times
    And all responses should equal 200
    And cache status should be "Hit"
    And FSM states should be:
      | CheckRequestCachePolicy |
      | PollCache               |
      | CheckCacheState         |
      | Response                |

  # =============================================================================
  # Cache Hit - Stale (Test Case 33)
  # =============================================================================

  @fsm @hit @stale
  Scenario: Stale cache hit returns cached response without revalidation
    Given cache policy is "Enabled"
    And request is cacheable
    And cache contains stale value 200
    When 1 request is made with value 100
    Then upstream should be called 0 times
    And all responses should equal 200
    And cache status should be "Hit"
    And FSM states should be:
      | CheckRequestCachePolicy |
      | PollCache               |
      | CheckCacheState         |
      | Response                |

  # =============================================================================
  # Cache Hit - Expired, No Dogpile Prevention (Test Cases 21, 22)
  # =============================================================================

  @fsm @hit @expired @no-dogpile
  Scenario: Expired cache triggers upstream call - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache contains expired value 200
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckCacheState          |
      | CheckConcurrency         |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | Response                 |

  @fsm @hit @expired @no-dogpile
  Scenario: Expired cache triggers upstream call - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache contains expired value 200
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should contain value 100
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckCacheState          |
      | CheckConcurrency         |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | UpdateCache              |
      | Response                 |

  # =============================================================================
  # Cache Hit - Expired, Dogpile Prevention: Proceed (Test Cases 31, 32)
  # =============================================================================

  @fsm @hit @expired @dogpile @proceed
  Scenario: Expired cache with permit - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache contains expired value 200
    And concurrency limit is 1
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckCacheState          |
      | CheckConcurrency         |
      | ConcurrentPollUpstream   |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | Response                 |

  @fsm @hit @expired @dogpile @proceed
  Scenario: Expired cache with permit - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache contains expired value 200
    And concurrency limit is 1
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should contain value 100
    And FSM states should be:
      | CheckRequestCachePolicy  |
      | PollCache                |
      | CheckCacheState          |
      | CheckConcurrency         |
      | ConcurrentPollUpstream   |
      | PollUpstream             |
      | UpstreamPolled           |
      | CheckResponseCachePolicy |
      | UpdateCache              |
      | Response                 |

  # =============================================================================
  # Cache Hit - Expired, Dogpile Prevention: Await OK (Test Case 29)
  # =============================================================================

  @fsm @hit @expired @dogpile @await @broadcast-ok
  Scenario: Expired cache with concurrent revalidation via broadcast
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache contains expired value 200
    And concurrency limit is 1
    When 3 requests are made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And cache should contain value 100
    And FSM states for each request should be:
      | Request 1                | Request 2                | Request 3                |
      | CheckRequestCachePolicy  | CheckRequestCachePolicy  | CheckRequestCachePolicy  |
      | PollCache                | PollCache                | PollCache                |
      | CheckCacheState          | CheckCacheState          | CheckCacheState          |
      | CheckConcurrency         | CheckConcurrency         | CheckConcurrency         |
      | ConcurrentPollUpstream   | ConcurrentPollUpstream   | ConcurrentPollUpstream   |
      | PollUpstream             | AwaitResponse            | AwaitResponse            |
      | UpstreamPolled           | Response                 | Response                 |
      | CheckResponseCachePolicy |                          |                          |
      | UpdateCache              |                          |                          |
      | Response                 |                          |                          |

  # =============================================================================
  # Cache Hit - Expired, Dogpile Prevention: Await Closed (Test Cases 24, 25)
  # =============================================================================

  @fsm @hit @expired @dogpile @await @broadcast-closed @allow.failed
  Scenario: Expired cache - waiter falls back when channel closed - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache contains expired value 200
    And concurrency limit is 1
    And broadcast channel will be closed
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  @fsm @hit @expired @dogpile @await @broadcast-closed @allow.failed
  Scenario: Expired cache - waiter falls back when channel closed - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache contains expired value 200
    And concurrency limit is 1
    And broadcast channel will be closed
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  # =============================================================================
  # Cache Hit - Expired, Dogpile Prevention: Await Lagged (Test Cases 27, 28)
  # =============================================================================

  @fsm @hit @expired @dogpile @await @broadcast-lagged @allow.failed
  Scenario: Expired cache - waiter falls back when channel lagged - response non-cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is non-cacheable
    And cache contains expired value 200
    And concurrency limit is 1
    And broadcast channel will lag
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  @fsm @hit @expired @dogpile @await @broadcast-lagged @allow.failed
  Scenario: Expired cache - waiter falls back when channel lagged - response cacheable
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache contains expired value 200
    And concurrency limit is 1
    And broadcast channel will lag
    When 2 requests are made with value 100
    Then upstream should be called 2 times
    And all responses should equal 100

  # =============================================================================
  # Concurrency Limit Variations
  # =============================================================================

  @fsm @dogpile @concurrency
  Scenario: Concurrency limit 3 allows parallel upstream calls
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And cache is empty
    And concurrency limit is 3
    When 3 requests are made with value 100
    Then upstream should be called 3 times
    And all responses should equal 100
    And FSM states for each request should be:
      | Request 1                | Request 2                | Request 3                |
      | CheckRequestCachePolicy  | CheckRequestCachePolicy  | CheckRequestCachePolicy  |
      | PollCache                | PollCache                | PollCache                |
      | CheckConcurrency         | CheckConcurrency         | CheckConcurrency         |
      | ConcurrentPollUpstream   | ConcurrentPollUpstream   | ConcurrentPollUpstream   |
      | PollUpstream             | PollUpstream             | PollUpstream             |
      | UpstreamPolled           | UpstreamPolled           | UpstreamPolled           |
      | CheckResponseCachePolicy | CheckResponseCachePolicy | CheckResponseCachePolicy |
      | UpdateCache              | UpdateCache              | UpdateCache              |
      | Response                 | Response                 | Response                 |
