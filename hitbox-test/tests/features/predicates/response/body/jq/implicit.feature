Feature: Response Body Jq Implicit Syntax

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @response @body @jq @implicit
  Scenario: Implicit syntax - simple boolean expression - response cached
    Given response predicates
      ```yaml
      - Body:
          jq: 'length == 3'
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

  @response @body @jq @implicit
  Scenario: Implicit syntax - boolean expression false - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq: 'length == 10'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @implicit
  Scenario: Implicit syntax - any() with array search - response cached
    Given response predicates
      ```yaml
      - Body:
          jq: 'any(.[]; .id == "journey-beyond-tomorrow")'
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

  @response @body @jq @implicit
  Scenario: Implicit syntax - complex boolean logic - response cached
    Given response predicates
      ```yaml
      - Body:
          jq: 'length >= 3 and .[0].id == "dimension-of-miracles"'
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
