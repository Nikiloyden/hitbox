Feature: Request Body Jq In Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @request @body @jq @in
  Scenario: Jq In - value in list - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.status"
            in: ["active", "pending"]
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-in-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"status":"active"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-in-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"status":"pending"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @in
  Scenario: Jq In - value not in list - request not cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.role"
            in: ["admin", "moderator"]
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-in-2
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"role":"user"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @jq @in
  Scenario: Jq In - number in list - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.priority"
            in: [1, 2, 3]
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-in-3
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"priority":1}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-in-3
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"priority":2}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @in
  Scenario: Jq In - nested field in list - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.user.level"
            in: ["beginner", "intermediate", "advanced"]
      ```
    And key extractors
      ```yaml
      - Method:
      - Path: "/v1/authors/{author_id}/books/{book_id}"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-in-4
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"user":{"level":"beginner","name":"alice"}}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-in-4
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"user":{"level":"advanced","name":"bob"}}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"
