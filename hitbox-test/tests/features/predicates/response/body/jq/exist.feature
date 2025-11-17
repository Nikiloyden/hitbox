Feature: Response Body Jq Exist Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @response @body @jq @exist
  Scenario: Jq Exist - field exists - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            exist: true
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

  @response @body @jq @exist
  Scenario: Jq Exist - field missing - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".non_existent_field"
            exist: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @exist
  Scenario: Jq Exist - null field counts as existing - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.pages"
            exist: true
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

  @response @body @jq @exist
  Scenario: Jq Exist - nested field exists - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.year"
            exist: true
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

  @response @body @jq @exist
  Scenario: Jq Exist - nested field missing - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.invalid"
            exist: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @exist
  Scenario: Jq Exist - array element exists - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[0]"
            exist: true
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

  @response @body @jq @exist
  Scenario: Jq Exist - array element missing - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[100]"
            exist: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @exist
  Scenario: Jq Exist - field in object without metadata - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.year"
            exist: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/dimension-of-miracles
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @exist
  Scenario: Jq Exist - multiple exist predicates - all must match
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            exist: true
      - Body:
          jq:
            expression: ".title"
            exist: true
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

  @response @body @jq @exist
  Scenario: Jq Exist - multiple predicates, one field missing - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            exist: true
      - Body:
          jq:
            expression: ".non_existent"
            exist: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
