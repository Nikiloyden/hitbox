Feature: Request Body Plain Starts Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @request @body @plain @starts
  Scenario: Body Starts - body starts with prefix - request cached
    Given request predicates
      ```yaml
      - Body:
          starts: '{"title"'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-starts-1
      Content-Type: application/json
      {"title":"Test Book","description":"Description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-starts-1
      Content-Type: application/json
      {"title":"Test Book","description":"Description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @starts
  Scenario: Body Starts - body doesn't start with prefix - request not cached
    Given request predicates
      ```yaml
      - Body:
          starts: "WRONG"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-starts-2
      Content-Type: application/json
      {"title":"Test Book"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @starts
  Scenario: Body Starts - empty prefix matches any body - request cached
    Given request predicates
      ```yaml
      - Body:
          starts: ""
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-starts-3
      Content-Type: application/json
      {"title":"Any content"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @plain @starts
  Scenario: Body Starts - case-sensitive matching - request not cached
    Given request predicates
      ```yaml
      - Body:
          starts: '{"TITLE"'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-starts-4
      Content-Type: application/json
      {"title":"Test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
