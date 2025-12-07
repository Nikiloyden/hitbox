Feature: Composition Backend with L1/L2 Refill

  Background:
    Given upstream response delay is 100ms
    And request delay between concurrent requests is 10ms
    And composition backend is enabled

  # =============================================================================
  # L1 Hit - Direct Return
  # =============================================================================

  @fsm @composition @l1-hit
  Scenario: L1 hit returns cached response without L2 access
    Given cache policy is "Enabled"
    And request is cacheable
    And refill policy is "Always"
    And L1 contains fresh value 200
    And L2 is empty
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should not be called
    And all responses should equal 200
    And cache status should be "Hit"
    And L1 should contain value 200
    And FSM states should be:
      | Initial                 |
      | CheckRequestCachePolicy |
      | PollCache               |
      | ConvertResponse         |
      | Response                |

  # =============================================================================
  # L2 Hit with L1 Refill
  # =============================================================================

  @fsm @composition @refill
  Scenario: L2 hit triggers L1 refill with RefillPolicy::Always
    Given cache policy is "Enabled"
    And request is cacheable
    And refill policy is "Always"
    And L1 is empty
    And L2 contains fresh value 200
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should not be called
    And all responses should equal 200
    And cache status should be "Hit"
    And L1 should contain value 200
    And L2 should contain value 200
    And FSM states should be:
      | Initial                 |
      | CheckRequestCachePolicy |
      | PollCache               |
      | UpdateCache             |
      | Response                |

  @fsm @composition @refill @never
  Scenario: L2 hit does not trigger L1 refill with RefillPolicy::Never
    Given cache policy is "Enabled"
    And request is cacheable
    And refill policy is "Never"
    And L1 is empty
    And L2 contains fresh value 200
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should not be called
    And all responses should equal 200
    And cache status should be "Hit"
    And L1 should be empty
    And L2 should contain value 200
    And FSM states should be:
      | Initial                 |
      | CheckRequestCachePolicy |
      | PollCache               |
      | ConvertResponse         |
      | Response                |

  # =============================================================================
  # Both L1 and L2 Miss
  # =============================================================================

  @fsm @composition @miss
  Scenario: Both L1 and L2 miss triggers upstream and populates both layers
    Given cache policy is "Enabled"
    And request is cacheable
    And response is cacheable
    And refill policy is "Always"
    And L1 is empty
    And L2 is empty
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should be called 1 time
    And all responses should equal 100
    And L1 should contain value 100
    And L2 should contain value 100
    And FSM states should be:
      | Initial                                       |
      | CheckRequestCachePolicy                       |
      | PollCache {concurrency.decision = disabled}   |
      | PollUpstream                                  |
      | CheckResponseCachePolicy                      |
      | UpdateCache                                   |
      | Response                                      |

  # =============================================================================
  # Stale L2 Hit - No Refill for Stale Data
  # =============================================================================

  @fsm @composition @stale
  Scenario: Stale L2 hit returns cached response without L1 refill
    Given cache policy is "Enabled"
    And request is cacheable
    And refill policy is "Always"
    And L1 is empty
    And L2 contains stale value 200
    And concurrency control is disabled
    When 1 request is made with value 100
    Then upstream should not be called
    And all responses should equal 200
    And cache status should be "Stale"
    # Stale data doesn't trigger refill - only fresh data does
    And L1 should be empty
    And L2 should contain value 200
    And FSM states should be:
      | Initial                 |
      | CheckRequestCachePolicy |
      | PollCache               |
      | HandleStale             |
      | Response                |
