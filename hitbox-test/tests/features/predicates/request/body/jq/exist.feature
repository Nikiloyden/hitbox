Feature: Request Body Jq Exist Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @request @body @jq @exist
  Scenario: Jq Exist - field exists - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.field"
            exist: true
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-exist-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"field":"value1","other":"data"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-exist-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"field":"value2","other":"different"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @exist
  Scenario: Jq Exist - field doesn't exist - request not cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.required_field"
            exist: true
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-exist-2
      Content-Type: application/json
      {"title":"Test Book","other":"data"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @jq @exist
  Scenario: Jq Exist - nested field exists - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.user.id"
            exist: true
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-exist-3
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"user":{"id":123,"name":"alice"}}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-exist-3
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"user":{"id":456,"name":"bob"}}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @exist
  Scenario: Jq Exist - array element exists - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".[0]"
            exist: true
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-exist-4
      Content-Type: application/json
      ["first","second"]
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-exist-4
      Content-Type: application/json
      ["different","values"]
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"
