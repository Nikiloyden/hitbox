Feature: Request Header Exist Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @request @header @exist
  Scenario: Header Exist - presence check caches request
    Given request predicates
      ```yaml
      - Header:
          Authorization:
            exist: true
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      Authorization: Bearer any-token-here
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      Authorization: Bearer different-token
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @header @exist
  Scenario: Header Exist - missing header not cached
    Given request predicates
      ```yaml
      - Header:
          Authorization:
            exist: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @header @exist
  Scenario: Header Exist - additional header doesn't affect cache decision
    Given request predicates
      ```yaml
      - Header:
          Authorization:
            exist: true
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      Authorization: Bearer token123
      User-Agent: Mozilla/5.0
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @header @exist
  Scenario: Header Exist - accepts any value
    Given request predicates
      ```yaml
      - Header:
          X-Trace-Id:
            exist: true
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      X-Trace-Id: trace-abc-123
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      X-Trace-Id: completely-different-value
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @header @exist
  Scenario: Header Exist - case-insensitive header name
    Given request predicates
      ```yaml
      - Header:
          Authorization:
            exist: true
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      authorization: Bearer token123
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
