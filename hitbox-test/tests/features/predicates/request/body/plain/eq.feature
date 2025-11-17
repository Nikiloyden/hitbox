Feature: Request Body Plain Eq Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @request @body @plain @eq
  Scenario: Body Eq - exact body match - request cached
    Given request predicates
      ```yaml
      - Body:
          eq: '{"title":"Test Book","description":"Test description"}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @eq
  Scenario: Body Eq - different body - request not cached
    Given request predicates
      ```yaml
      - Body:
          eq: '{"title":"Test Book","description":"Test description"}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-2
      Content-Type: application/json
      {"title":"Different","description":"Different content"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @eq
  Scenario: Body Eq - case-sensitive matching - request not cached
    Given request predicates
      ```yaml
      - Body:
          eq: '{"Title":"Test Book","description":"Test description"}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-3
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @eq
  Scenario: Body Eq - whitespace sensitive - request not cached
    Given request predicates
      ```yaml
      - Body:
          eq: '{"title":"Test Book","description":"Test description"}  '
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-4
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @eq
  Scenario: Body Eq - partial match not sufficient - request not cached
    Given request predicates
      ```yaml
      - Body:
          eq: '{"title":"Test Book"}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-5
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @eq
  Scenario: Body Eq - multiple eq predicates - all must match
    Given request predicates
      ```yaml
      - Body:
          eq: '{"title":"Test Book","description":"Test description"}'
      - Body:
          eq: '{"title":"Test Book","description":"Test description"}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-6
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-6
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @eq
  Scenario: Body Eq - multiple predicates, one doesn't match - request not cached
    Given request predicates
      ```yaml
      - Body:
          eq: '{"title":"Test Book","description":"Test description"}'
      - Body:
          eq: "WRONG"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-eq-7
      Content-Type: application/json
      {"title":"Test Book","description":"Test description"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
