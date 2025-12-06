Feature: Cache FSM Behavior

  Background:
    Given upstream response delay is 100ms
    And request delay between concurrent requests is 10ms

  # =============================================================================
  # Cache Disabled
  # =============================================================================

  @fsm @disabled
  Scenario: Cache disabled - direct upstream call
    Given cache policy is "Disabled"
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And FSM states should be:
      | Initial      |
      | PollUpstream |
      | Response     |

  # =============================================================================
  # Request Non-Cacheable
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
      | Initial                 |
      | CheckRequestCachePolicy |
      | PollUpstream            |
      | Response                |

  # =============================================================================
  # Cache Miss - No Dogpile Prevention
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
      | Initial                                       |
      | CheckRequestCachePolicy                       |
      | PollCache {concurrency.decision = disabled}   |
      | PollUpstream                                  |
      | CheckResponseCachePolicy                      |
      | Response                                      |

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
      | Initial                                       |
      | CheckRequestCachePolicy                       |
      | PollCache {concurrency.decision = disabled}   |
      | PollUpstream                                  |
      | CheckResponseCachePolicy                      |
      | UpdateCache                                   |
      | Response                                      |

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Proceed with Permit
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
      | Initial                                      |
      | CheckRequestCachePolicy                      |
      | PollCache {concurrency.decision = proceed}   |
      | PollUpstream                                 |
      | CheckResponseCachePolicy                     |
      | Response                                     |

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
      | Initial                                      |
      | CheckRequestCachePolicy                      |
      | PollCache {concurrency.decision = proceed}   |
      | PollUpstream                                 |
      | CheckResponseCachePolicy                     |
      | UpdateCache                                  |
      | Response                                     |

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Await Broadcast OK
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
      | Request 1                                    | Request 2                                   | Request 3                                   |
      | Initial                                      | Initial                                     | Initial                                     |
      | CheckRequestCachePolicy                      | CheckRequestCachePolicy                     | CheckRequestCachePolicy                     |
      | PollCache {concurrency.decision = proceed}   | PollCache {concurrency.decision = await}    | PollCache {concurrency.decision = await}    |
      | PollUpstream                                 | AwaitResponse                               | AwaitResponse                               |
      | CheckResponseCachePolicy                     | Response                                    | Response                                    |
      | UpdateCache                                  |                                             |                                             |
      | Response                                     |                                             |                                             |

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
      | Request 1                                    | Request 2                                   | Request 3                                   |
      | Initial                                      | Initial                                     | Initial                                     |
      | CheckRequestCachePolicy                      | CheckRequestCachePolicy                     | CheckRequestCachePolicy                     |
      | PollCache {concurrency.decision = proceed}   | PollCache {concurrency.decision = await}    | PollCache {concurrency.decision = await}    |
      | PollUpstream                                 | AwaitResponse                               | AwaitResponse                               |
      | CheckResponseCachePolicy                     | PollUpstream                                | PollUpstream                                |
      | Response                                     | CheckResponseCachePolicy                    | CheckResponseCachePolicy                    |
      |                                              | Response                                    | Response                                    |

  # =============================================================================
  # Cache Miss - Dogpile Prevention: Await Broadcast Closed
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
  # Cache Miss - Dogpile Prevention: Await Broadcast Lagged
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
  # Cache Hit - Actual/Fresh
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
      | Initial                 |
      | CheckRequestCachePolicy |
      | PollCache               |
      | ConvertResponse         |
      | Response                |

  # =============================================================================
  # Cache Hit - Stale
  # =============================================================================

  @fsm @hit @stale
  Scenario: Stale cache hit returns cached response without revalidation
    Given cache policy is "Enabled"
    And request is cacheable
    And cache contains stale value 200
    When 1 request is made with value 100
    Then upstream should be called 0 times
    And all responses should equal 200
    And cache status should be "Stale"
    And FSM states should be:
      | Initial                 |
      | CheckRequestCachePolicy |
      | PollCache               |
      | HandleStale             |
      | Response                |

  # =============================================================================
  # Cache Hit - Expired, No Dogpile Prevention
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
      | Initial                                       |
      | CheckRequestCachePolicy                       |
      | PollCache {concurrency.decision = disabled}   |
      | PollUpstream                                  |
      | CheckResponseCachePolicy                      |
      | Response                                      |

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
      | Initial                                       |
      | CheckRequestCachePolicy                       |
      | PollCache {concurrency.decision = disabled}   |
      | PollUpstream                                  |
      | CheckResponseCachePolicy                      |
      | UpdateCache                                   |
      | Response                                      |

  # =============================================================================
  # Cache Hit - Expired, Dogpile Prevention: Proceed
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
      | Initial                                      |
      | CheckRequestCachePolicy                      |
      | PollCache {concurrency.decision = proceed}   |
      | PollUpstream                                 |
      | CheckResponseCachePolicy                     |
      | Response                                     |

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
      | Initial                                      |
      | CheckRequestCachePolicy                      |
      | PollCache {concurrency.decision = proceed}   |
      | PollUpstream                                 |
      | CheckResponseCachePolicy                     |
      | UpdateCache                                  |
      | Response                                     |

  # =============================================================================
  # Cache Hit - Expired, Dogpile Prevention: Await OK
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
      | Request 1                                    | Request 2                                   | Request 3                                   |
      | Initial                                      | Initial                                     | Initial                                     |
      | CheckRequestCachePolicy                      | CheckRequestCachePolicy                     | CheckRequestCachePolicy                     |
      | PollCache {concurrency.decision = proceed}   | PollCache {concurrency.decision = await}    | PollCache {concurrency.decision = await}    |
      | PollUpstream                                 | AwaitResponse                               | AwaitResponse                               |
      | CheckResponseCachePolicy                     | Response                                    | Response                                    |
      | UpdateCache                                  |                                             |                                             |
      | Response                                     |                                             |                                             |

  # =============================================================================
  # Cache Hit - Expired, Dogpile Prevention: Await Closed
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
  # Cache Hit - Expired, Dogpile Prevention: Await Lagged
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
      | Request 1                                    | Request 2                                    | Request 3                                    |
      | Initial                                      | Initial                                      | Initial                                      |
      | CheckRequestCachePolicy                      | CheckRequestCachePolicy                      | CheckRequestCachePolicy                      |
      | PollCache {concurrency.decision = proceed}   | PollCache {concurrency.decision = proceed}   | PollCache {concurrency.decision = proceed}   |
      | PollUpstream                                 | PollUpstream                                 | PollUpstream                                 |
      | CheckResponseCachePolicy                     | CheckResponseCachePolicy                     | CheckResponseCachePolicy                     |
      | UpdateCache                                  | UpdateCache                                  | UpdateCache                                  |
      | Response                                     | Response                                     | Response                                     |
