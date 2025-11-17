Feature: Response Body Jq In Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @integration
  Scenario: Jq In - field value in list - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            in: ["victim-prime", "immortality-inc", "dimension-of-miracles"]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @integration
  Scenario: Jq In - field value not in list - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            in: ["immortality-inc", "dimension-of-miracles"]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @integration
  Scenario: Jq In - empty list - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            in: []
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @integration
  Scenario: Jq In - single value in list - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            in: ["victim-prime"]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @integration
  Scenario: Jq In - number value in list - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.year"
            in: [1987, 1962, 1968]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @integration
  Scenario: Jq In - number value not in list - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.year"
            in: [1962, 1968, 2000]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @integration
  Scenario: Jq In - mixed value types in list - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".title"
            in: ["Victim Prime", 123, true, null]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @integration
  Scenario: Jq In - missing field - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".non_existent"
            in: ["value1", "value2"]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @integration
  Scenario: Jq In - nested field value in list - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.pages"
            in: [192, 256, 320]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @integration
  Scenario: Jq In - array element value in list - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[0].id"
            in: ["dimension-of-miracles", "immortality-inc"]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @integration
  Scenario: Jq In - multiple in predicates - all must match
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            in: ["victim-prime", "immortality-inc"]
      - Body:
          jq:
            expression: ".metadata.year"
            in: [1987, 1962]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @integration
  Scenario: Jq In - multiple predicates, one not in list - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            in: ["victim-prime", "immortality-inc"]
      - Body:
          jq:
            expression: ".metadata.year"
            in: [1962, 1968]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
