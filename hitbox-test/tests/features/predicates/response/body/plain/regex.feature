Feature: Response Body Regex Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @response @body @plain @regex
  Scenario: Body Regex - simple pattern match - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "Victim Prime"
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

  @response @body @plain @regex
  Scenario: Body Regex - pattern doesn't match - response not cached
    Given response predicates
      ```yaml
      - Body:
          regex: "DOES_NOT_EXIST"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @regex
  Scenario: Body Regex - wildcard pattern - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "Victim.*Prime"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - character class pattern - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "robert-[a-z]+"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - optional pattern - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "Victim\\s?Prime"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - anchored pattern start - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: '^\{"id"'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - anchored pattern end - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: '"pages":256\}\}$'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - alternation pattern - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "(Victim|Hunter)"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - case-sensitive matching - response not cached
    Given response predicates
      ```yaml
      - Body:
          regex: "VICTIM PRIME"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @regex
  Scenario: Body Regex - multiline pattern - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "published in 1987.*sequel"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - digit pattern - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "\\d{4}"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @regex
  Scenario: Body Regex - streaming body collection - response cached
    Given response predicates
      ```yaml
      - Body:
          regex: "Victim Prime"
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

  @response @body @plain @regex
  Scenario: Body Regex - multiple regex predicates - all must match
    Given response predicates
      ```yaml
      - Body:
          regex: "Victim Prime"
      - Body:
          regex: "robert-sheckley"
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

  @response @body @plain @regex
  Scenario: Body Regex - multiple predicates, one doesn't match - response not cached
    Given response predicates
      ```yaml
      - Body:
          regex: "Victim Prime"
      - Body:
          regex: "DOES_NOT_MATCH"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records
