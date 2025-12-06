Feature: Request Body Jq Eq Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @request @body @jq @eq
  Scenario: Jq Eq - JSON field equals value - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.field"
            eq: "test-value"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"field":"test-value","other":"data"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-1
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"field":"test-value","other":"data"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @eq
  Scenario: Jq Eq - JSON field not equals value - request not cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.field"
            eq: "expected-value"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-2
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"field":"wrong-value"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @jq @eq
  Scenario: Jq Eq - nested field extraction - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.inner.field_one"
            eq: "value_one"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-3
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"inner":{"field_one":"value_one","field_two":"value_two"}}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-3
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"inner":{"field_one":"value_one","field_two":"value_two"}}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @eq
  Scenario: Jq Eq - array index access - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.items[1].key"
            eq: "my-key-01"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-4
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"items":[{"key":"my-key-00"},{"key":"my-key-01"}]}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-4
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"items":[{"key":"my-key-00"},{"key":"my-key-01"}]}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @eq
  Scenario: Jq Eq - number value match - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.count"
            eq: 42
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-5
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"count":42}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @jq @eq
  Scenario: Jq Eq - boolean value match - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.active"
            eq: true
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-6
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"active":true}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @jq @eq
  Scenario: Jq Eq - array length check - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata | length"
            eq: 3
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-7
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":[1,2,3]}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-7
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":[1,2,3]}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @eq
  Scenario: Jq Expression - check array length - request cached
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: '.metadata | length == 3'
            eq: true
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-8
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":["a","b","c"]}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-8
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":["a","b","c"]}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @jq @eq
  Scenario: Jq Eq - multiple jq predicates - all must match
    Given request predicates
      ```yaml
      - Body:
          jq:
            expression: ".metadata.user"
            eq: "alice"
      - Body:
          jq:
            expression: ".metadata.role"
            eq: "admin"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-9
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"user":"alice","role":"admin"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-jq-eq-9
      Content-Type: application/json
      {"title":"Test Book","description":"Test description","metadata":{"user":"alice","role":"admin"}}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"
