Feature: Request Body Plain Limit Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @request @body @plain @limit
  Scenario: Body Limit - body within limit - request cached
    Given request predicates
      ```yaml
      - Body:
          limit: 100
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-limit-1
      Content-Type: application/json
      {"title":"Short"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-limit-1
      Content-Type: application/json
      {"title":"Short"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @limit
  Scenario: Body Limit - body exceeds limit - request not cached
    Given request predicates
      ```yaml
      - Body:
          limit: 10
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-limit-2
      Content-Type: application/json
      {"title":"This is a very long title that exceeds the limit"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @limit
  Scenario: Body Limit - empty body within limit - request cached
    Given request predicates
      ```yaml
      - Body:
          limit: 100
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-limit-3
      Content-Type: application/json
      {"title":"Test Book"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @plain @limit
  Scenario: Body Limit - exact limit boundary - request cached
    Given request predicates
      ```yaml
      - Body:
          limit: 50
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-limit-4
      Content-Type: application/json
      {"title":"Test Book","t":"12345678"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
