Feature: Request Path Predicate Functionality

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @request @path
  Scenario: Path Eq operation - exact path match cached
    Given request predicates
      ```yaml
      - Path: /v1/authors/robert-sheckley/books/victim-prime
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

  @request @path
  Scenario: Path Eq operation - partial path match not cached
    Given request predicates
      ```yaml
      - Path: /v1/authors/robert-sheckley/books/victim-prime
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @path
  Scenario: Path Eq operation - path with path parameter cached
    Given request predicates
      ```yaml
      - Path: /v1/authors/{author_id}/books/{book_id}
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

  @request @path
  Scenario: Path Eq operation - path with trailing slash doesn't match path without trailing slash
    Given request predicates
      ```yaml
      - Path: /v1/authors/robert-sheckley/books/victim-prime/
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @path
  Scenario: Path Eq operation - path case sensitivity
    Given request predicates
      ```yaml
      - Path: /v1/authors/robert-sheckley/books/victim-prime
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/Robert-Sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @path
  Scenario: Path Eq operation - path with encoded characters
    Given request predicates
      ```yaml
      - Path: /v1/authors/{author_id}/books/{book_id}
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert%20sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert%20sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"
    And cache has 1 records

  @request @path @in
  Scenario: Path In operation - first path matches
    Given request predicates
      ```yaml
      - Path:
          in:
            - /v1/authors/robert-sheckley/books
            - /v1/authors/isaac-asimov/books
            - /v1/authors/arthur-clarke/books
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

  @request @path @in
  Scenario: Path In operation - middle path matches
    Given request predicates
      ```yaml
      - Path:
          in:
            - /v1/authors/robert-sheckley/books
            - /v1/authors/isaac-asimov/books
            - /v1/authors/arthur-clarke/books
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/isaac-asimov/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @path @in
  Scenario: Path In operation - last path matches
    Given request predicates
      ```yaml
      - Path:
          in:
            - /v1/authors/robert-sheckley/books
            - /v1/authors/isaac-asimov/books
            - /v1/authors/roger-zelazny/books
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/roger-zelazny/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @path @in
  Scenario: Path In operation - no path matches
    Given request predicates
      ```yaml
      - Path:
          in:
            - /v1/authors/robert-sheckley/books
            - /v1/authors/isaac-asimov/books
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/roger-zelazny/books
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @path @in
  Scenario: Path In operation - with path parameters
    Given request predicates
      ```yaml
      - Path:
          in:
            - /v1/authors/{author_id}/books
            - /v1/authors/{author_id}/books/{book_id}
      ```
    And key extractors
      ```yaml
      - Path: /v1/authors/{author_id}/books/{book_id}
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
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 2 records
