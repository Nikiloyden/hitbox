Feature: Response Body Jq Complex Expressions

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @response @body @jq @complex
  Scenario: Array mapping - extract all author values - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[].author"
            eq: ["robert-sheckley", "robert-sheckley", "robert-sheckley"]
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

  @response @body @jq @complex
  Scenario: Array mapping - extract all ids - response cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[].id"
            eq: ["dimension-of-miracles", "immortality-inc", "journey-beyond-tomorrow"]
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

  @response @body @jq @complex
  Scenario: Array mapping - wrong values - response not cached
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[].id"
            eq: ["wrong-id-1", "wrong-id-2", "wrong-id-3"]
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @jq @complex
  Scenario: Array mapping with exist - check if mapping produces results
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[].title"
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

  @response @body @jq @complex
  Scenario: Array mapping with in - check if all values in list
    Given response predicates
      ```yaml
      - Body:
          jq:
            expression: ".[].author"
            in: [["robert-sheckley", "robert-sheckley", "robert-sheckley"], ["isaac-asimov"]]
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
