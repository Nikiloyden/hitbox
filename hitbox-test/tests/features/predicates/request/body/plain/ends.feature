Feature: Request Body Plain Ends Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @request @body @plain @ends
  Scenario: Body Ends - body ends with suffix - request cached
    Given request predicates
      ```yaml
      - Body:
          ends: '"end"}'
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-ends-1
      Content-Type: application/json
      {"title":"Test Book","data":"test","suffix":"end"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-ends-1
      Content-Type: application/json
      {"title":"Test Book","data":"test","suffix":"end"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @request @body @plain @ends
  Scenario: Body Ends - body doesn't end with suffix - request not cached
    Given request predicates
      ```yaml
      - Body:
          ends: "WRONG"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-ends-2
      Content-Type: application/json
      {"title":"Test Book","data":"test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @request @body @plain @ends
  Scenario: Body Ends - empty suffix matches any body - request cached
    Given request predicates
      ```yaml
      - Body:
          ends: ""
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-ends-3
      Content-Type: application/json
      {"title":"Test Book","any":"content"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @request @body @plain @ends
  Scenario: Body Ends - case-sensitive matching - request not cached
    Given request predicates
      ```yaml
      - Body:
          ends: "TEST"
      ```
    When execute request
      ```hurl
      POST http://localhost/v1/authors/robert-sheckley/books/test-ends-4
      Content-Type: application/json
      {"title":"Test Book","value":"test"}
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
