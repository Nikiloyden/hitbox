Feature: Response Body Limit Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @response @body @plain @limit
  Scenario: Body Limit - body within size limit - response cached
    Given response predicates
      ```yaml
      - Body:
          limit: 500
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

  @response @body @plain @limit
  Scenario: Body Limit - body exceeds size limit - response not cached
    Given response predicates
      ```yaml
      - Body:
          limit: 100
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @limit
  Scenario: Body Limit - body exactly at limit - response cached
    Given response predicates
      ```yaml
      - Body:
          limit: 311
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @limit
  Scenario: Body Limit - body one byte over limit - response not cached
    Given response predicates
      ```yaml
      - Body:
          limit: 310
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @limit
  Scenario: Body Limit - very large limit - response cached
    Given response predicates
      ```yaml
      - Body:
          limit: 1000000
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @limit
  Scenario: Body Limit - zero limit allows only empty body - response not cached
    Given response predicates
      ```yaml
      - Body:
          limit: 0
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @limit
  Scenario: Body Limit - streaming body with size hint - response cached
    Given response predicates
      ```yaml
      - Body:
          limit: 500
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime?streaming=true
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @limit
  Scenario: Body Limit - streaming body exceeds limit - response not cached
    Given response predicates
      ```yaml
      - Body:
          limit: 100
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime?streaming=true
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @limit
  Scenario: Body Limit - multiple limit predicates - all must match
    Given response predicates
      ```yaml
      - Body:
          limit: 500
      - Body:
          limit: 1000
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @limit
  Scenario: Body Limit - multiple predicates, one limit exceeded - response not cached
    Given response predicates
      ```yaml
      - Body:
          limit: 500
      - Body:
          limit: 100
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @limit
  Scenario: Body Limit - combined with other predicates - both must match
    Given response predicates
      ```yaml
      - Body:
          limit: 500
      - Body:
          contains: "Victim Prime"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
