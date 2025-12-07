Feature: Response Body Jq Eq Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @response @body @jq @eq
  Scenario: Jq Eq - JSON field equals value - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            eq: "victim-prime"
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

  @response @body @jq @eq
  Scenario: Jq Eq - JSON field not equals value - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            eq: "wrong-id"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @eq
  Scenario: Jq Eq - nested field extraction - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.year"
            eq: 1987
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

  @response @body @jq @eq
  Scenario: Jq Eq - array index access - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[0].author"
            eq: "robert-sheckley"
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

  @response @body @jq @eq
  Scenario: Jq Eq - array index access - value mismatch - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[0].author"
            eq: "wrong-author"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @eq
  Scenario: Jq Eq - string field comparison - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".title"
            eq: "Victim Prime"
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

  @response @body @jq @eq
  Scenario: Jq Eq - array length check - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: "length"
            eq: 3
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

  @response @body @jq @eq
  Scenario: Jq Eq - array length check - value mismatch - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: "length"
            eq: 10
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @eq
  Scenario: Jq Eq - multiple jq predicates - all must match
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            eq: "victim-prime"
      - Body:
          jq:
            expression: ".title"
            eq: "Victim Prime"
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

  @response @body @jq @eq
  Scenario: Jq Eq - multiple predicates, one doesn't match - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".id"
            eq: "victim-prime"
      - Body:
          jq:
            expression: ".title"
            eq: "Wrong Title"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @eq
  Scenario: Jq Expression - check array length - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: 'length == 3'
            eq: true
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

  @response @body @jq @eq
  Scenario: Jq Expression - array length mismatch - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: 'length == 10'
            eq: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @eq
  Scenario: Jq Expression - count items matching condition - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: '([.[] | select(.author == "robert-sheckley")] | length) >= 3'
            eq: true
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

  @response @body @jq @eq
  Scenario: Jq Expression - check first item by id - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: '.[0].id == "dimension-of-miracles"'
            eq: true
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

  @response @body @jq @eq
  Scenario: Jq Expression - complex boolean logic with multiple conditions - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: 'length >= 3 and .[0].id == "dimension-of-miracles" and .[2].metadata != null and .[2].metadata.year >= 1960'
            eq: true
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

  @response @body @jq @eq
  Scenario: Jq Expression - find value in array with any - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: 'any(.[]; .id == "journey-beyond-tomorrow")'
            eq: true
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

  @response @body @jq @eq
  Scenario: Jq Expression - find value in array with any - no match - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: 'any(.[]; .id == "non-existent-book")'
            eq: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
