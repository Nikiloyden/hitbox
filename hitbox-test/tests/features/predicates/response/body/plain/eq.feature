Feature: Response Body Eq Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10s
      ```

  @response @body @plain @eq
  Scenario: Body Eq - exact body match - response cached
    Given response predicates
      ```yaml
      - Body:
          eq: '{"id":"victim-prime","author":"robert-sheckley","title":"Victim Prime","description":"Victim Prime is a science fiction novel by American writer Robert Sheckley,\npublished in 1987. It is the sequel to 1953''s \"Seventh Victim\",\nand is followed by 1988''s Hunter/Victim.\n","metadata":{"year":1987,"pages":256}}'
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

  @response @body @plain @eq
  Scenario: Body Eq - different body - response not cached
    Given response predicates
      ```yaml
      - Body:
          eq: '{"id":"wrong","author":"wrong","title":"Wrong","description":"Wrong"}'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @eq
  Scenario: Body Eq - streaming body collection - response cached
    Given response predicates
      ```yaml
      - Body:
          eq: '{"id":"victim-prime","author":"robert-sheckley","title":"Victim Prime","description":"Victim Prime is a science fiction novel by American writer Robert Sheckley,\npublished in 1987. It is the sequel to 1953''s \"Seventh Victim\",\nand is followed by 1988''s Hunter/Victim.\n","metadata":{"year":1987,"pages":256}}'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime?streaming=true
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime?streaming=true
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @response @body @plain @eq
  Scenario: Body Eq - case-sensitive matching - response not cached
    Given response predicates
      ```yaml
      - Body:
          eq: '{"ID":"victim-prime","author":"robert-sheckley","title":"Victim Prime","description":"Victim Prime is a science fiction novel by American writer Robert Sheckley,\npublished in 1987. It is the sequel to 1953''s \"Seventh Victim\",\nand is followed by 1988''s Hunter/Victim.\n"}'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @eq
  Scenario: Body Eq - whitespace sensitive - response not cached
    Given response predicates
      ```yaml
      - Body:
          eq: '{"id":"victim-prime","author":"robert-sheckley","title":"Victim Prime","description":"Victim Prime is a science fiction novel by American writer Robert Sheckley,\npublished in 1987. It is the sequel to 1953''s \"Seventh Victim\",\nand is followed by 1988''s Hunter/Victim.\n"}  '
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @eq
  Scenario: Body Eq - partial match not sufficient - response not cached
    Given response predicates
      ```yaml
      - Body:
          eq: '{"id":"victim-prime"}'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @eq
  Scenario: Body Eq - multiple eq predicates - all must match
    Given response predicates
      ```yaml
      - Body:
          eq: '{"id":"victim-prime","author":"robert-sheckley","title":"Victim Prime","description":"Victim Prime is a science fiction novel by American writer Robert Sheckley,\npublished in 1987. It is the sequel to 1953''s \"Seventh Victim\",\nand is followed by 1988''s Hunter/Victim.\n","metadata":{"year":1987,"pages":256}}'
      - Body:
          eq: '{"id":"victim-prime","author":"robert-sheckley","title":"Victim Prime","description":"Victim Prime is a science fiction novel by American writer Robert Sheckley,\npublished in 1987. It is the sequel to 1953''s \"Seventh Victim\",\nand is followed by 1988''s Hunter/Victim.\n","metadata":{"year":1987,"pages":256}}'
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

  @response @body @plain @eq
  Scenario: Body Eq - multiple predicates, one doesn't match - response not cached
    Given response predicates
      ```yaml
      - Body:
          eq: '{"id":"victim-prime","author":"robert-sheckley","title":"Victim Prime","description":"Victim Prime is a science fiction novel by American writer Robert Sheckley,\npublished in 1987. It is the sequel to 1953''s \"Seventh Victim\",\nand is followed by 1988''s Hunter/Victim.\n"}'
      - Body:
          eq: "WRONG"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
