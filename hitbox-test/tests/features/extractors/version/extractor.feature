Feature: Request Version Cache Key Extractor

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @extractor @version
  Scenario: Extract HTTP version for cache key
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Version:
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then cache key exists
      ```
      version: "HTTP/1.1"
      ```

  @extractor @version
  Scenario: Version extractor combined with method extractor
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Method:
      - Version:
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then cache key exists
      ```
      method: "GET"
      version: "HTTP/1.1"
      ```

  @extractor @version
  Scenario: Version extractor combined with path extractor
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      - Version:
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then cache key exists
      ```
      author_id: "robert-sheckley"
      book_id: "victim-prime"
      version: "HTTP/1.1"
      ```

  @extractor @version
  Scenario: Different requests with same version share cache entry
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Version:
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
      GET http://localhost/v1/authors/isaac-asimov/books/foundation
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @extractor @version
  Scenario: Version extractor with path creates separate cache entries for different paths
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      - Version:
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
      GET http://localhost/v1/authors/isaac-asimov/books/foundation
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 2 records
