@serial
Feature: Stale-While-Revalidate (SWR) Policy

  Background:
    Given offload revalidation is enabled

  @stale @revalidation
  Scenario: Basic cache hit verification
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
        stale: 5s
        policy:
          stale: OffloadRevalidate
      ```
    # First request - cache miss
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

    # Second request - should be cache hit (no sleep, no time manipulation)
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @stale @revalidation
  Scenario: Stale data returned immediately while background revalidation occurs
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 300ms
        stale: 100ms
        policy:
          stale: OffloadRevalidate
      ```
    # First request - cache miss
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

    # Second request within stale mark - cache hit
    When sleep 50ms
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

    # Third request after stale mark but within TTL - returns stale, triggers revalidation
    When sleep 100ms
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "STALE"

    # Wait for background revalidation to complete
    When wait for background tasks

    # Fourth request - should be fresh from revalidation
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @stale @revalidation
  Scenario: Default Return policy - stale data returned without revalidation
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 300ms
        stale: 100ms
        policy:
          stale: Return
      ```
    # First request - cache miss
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

    # Second request after stale mark but within TTL
    When sleep 150ms
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "STALE"

    # Wait to ensure no background task runs
    When wait for background tasks

    # Third request - still stale (no revalidation occurred)
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "STALE"

  @stale @revalidation
  Scenario: Revalidate policy - blocks until fresh data is fetched
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 300ms
        stale: 100ms
        policy:
          stale: Revalidate
      ```
    # First request - cache miss
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

    # Second request after stale mark - should revalidate synchronously
    When sleep 150ms
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    # Should be MISS because we fetched fresh data synchronously
    And response header "X-Cache-Status" is "MISS"

  @stale @revalidation
  Scenario: Expired data beyond TTL - cache miss
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 200ms
        stale: 100ms
        policy:
          stale: OffloadRevalidate
      ```
    # First request - cache miss
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

    # Request after TTL - should be expired
    When sleep 250ms
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
