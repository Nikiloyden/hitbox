Feature: Request Body Plain Contains Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @request @body @plain @contains
  Scenario: Body Contains - body contains text - request cached
    Given request predicates
      ```yaml
      - Body:
          contains: "important-data"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-1
      Content-Type: application/json
      {"title":"Test Book","description":"This contains important-data in the middle"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-1
      Content-Type: application/json
      {"title":"Test Book","description":"This contains important-data in the middle"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @contains
  Scenario: Body Contains - body doesn't contain text - request not cached
    Given request predicates
      ```yaml
      - Body:
          contains: "missing-text"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-2
      Content-Type: application/json
      {"title":"Test Book","description":"Different content"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @contains
  Scenario: Body Contains - empty pattern matches any body - request cached
    Given request predicates
      ```yaml
      - Body:
          contains: ""
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-3
      Content-Type: application/json
      {"title":"Any content"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-3
      Content-Type: application/json
      {"title":"Any content"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @contains
  Scenario: Body Contains - pattern at beginning of body - request cached
    Given request predicates
      ```yaml
      - Body:
          contains: '{"title"'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-4
      Content-Type: application/json
      {"title":"Test Book","description":"Description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @plain @contains
  Scenario: Body Contains - pattern at end of body - request cached
    Given request predicates
      ```yaml
      - Body:
          contains: 'description":"End"}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-5
      Content-Type: application/json
      {"title":"Test","description":"End"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @plain @contains
  Scenario: Body Contains - case-sensitive matching - request not cached
    Given request predicates
      ```yaml
      - Body:
          contains: "IMPORTANT"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-6
      Content-Type: application/json
      {"title":"Test","note":"important data"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @contains
  Scenario: Body Contains - multiple contains predicates - all must match
    Given request predicates
      ```yaml
      - Body:
          contains: "alpha"
      - Body:
          contains: "beta"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-7
      Content-Type: application/json
      {"title":"Test Book","data":"alpha beta gamma"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-7
      Content-Type: application/json
      {"title":"Test Book","data":"alpha beta gamma"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @contains
  Scenario: Body Contains - one predicate doesn't match - request not cached
    Given request predicates
      ```yaml
      - Body:
          contains: "alpha"
      - Body:
          contains: "missing"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-contains-8
      Content-Type: application/json
      {"title":"Test Book","data":"alpha beta gamma"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
