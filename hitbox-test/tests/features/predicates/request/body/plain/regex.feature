Feature: Request Body Plain Regex Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @request @body @plain @regex
  Scenario: Body Regex - pattern matches - request cached
    Given request predicates
      ```yaml
      - Body:
          regex: '"id":"[a-z0-9-]+"'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-regex-1
      Content-Type: application/json
      {"id":"test-book-123","title":"Test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-regex-1
      Content-Type: application/json
      {"id":"test-book-123","title":"Test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @regex
  Scenario: Body Regex - pattern doesn't match - request not cached
    Given request predicates
      ```yaml
      - Body:
          regex: '"author":"[0-9]+"'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-regex-2
      Content-Type: application/json
      {"author":"robert-sheckley","title":"Test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @regex
  Scenario: Body Regex - numeric pattern match - request cached
    Given request predicates
      ```yaml
      - Body:
          regex: '"count":\s*\d+'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-regex-3
      Content-Type: application/json
      {"count":42,"title":"Test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @plain @regex
  Scenario: Body Regex - email pattern match - request cached
    Given request predicates
      ```yaml
      - Body:
          regex: '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-regex-4
      Content-Type: application/json
      {"title":"Test Book","email":"test@example.com","name":"Test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
