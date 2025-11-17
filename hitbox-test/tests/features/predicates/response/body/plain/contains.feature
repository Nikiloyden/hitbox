Feature: Response Body Contains Predicate

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @response @body @plain @contains
  Scenario: Body Contains - body contains text - response cached
    Given response predicates
      ```yaml
      - Body:
          contains: "robert-sheckley"
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

  @response @body @plain @contains
  Scenario: Body Contains - body doesn't contain text - response not cached
    Given response predicates
      ```yaml
      - Body:
          contains: "isaac-asimov"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @contains
  Scenario: Body Contains - empty pattern matches any body - response cached
    Given response predicates
      ```yaml
      - Body:
          contains: ""
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

  @response @body @plain @contains
  Scenario: Body Contains - pattern at beginning of body - response cached
    Given response predicates
      ```yaml
      - Body:
          contains: '{"id"'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @contains
  Scenario: Body Contains - pattern at end of body - response cached
    Given response predicates
      ```yaml
      - Body:
          contains: '"pages":256}}'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @contains
  Scenario: Body Contains - pattern in middle of body - response cached
    Given response predicates
      ```yaml
      - Body:
          contains: "Victim Prime"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @contains
  Scenario: Body Contains - pattern spanning chunk boundaries - response cached
    Given response predicates
      ```yaml
      - Body:
          contains: 'er Robert She'
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

  @response @body @plain @contains
  Scenario: Body Contains - case-sensitive matching - response not cached
    Given response predicates
      ```yaml
      - Body:
          contains: "VICTIM PRIME"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @contains
  Scenario: Body Contains - pattern with special characters - response cached
    Given response predicates
      ```yaml
      - Body:
          contains: '"author":"robert-sheckley"'
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @contains
  Scenario: Body Contains - multiple contains predicates - all must match
    Given response predicates
      ```yaml
      - Body:
          contains: "robert-sheckley"
      - Body:
          contains: "Victim Prime"
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

  @response @body @plain @contains
  Scenario: Body Contains - multiple patterns, one doesn't match - response not cached
    Given response predicates
      ```yaml
      - Body:
          contains: "robert-sheckley"
      - Body:
          contains: "Foundation"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books/victim-prime
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 0 records

  @response @body @plain @contains @allow.failed
  Scenario: Body Contains - PNG magic bytes (binary data) - UNSUPPORTED (serde-saphyr limitation)
    # FIXME: Currently unsupported due to serde-saphyr issue with !!binary tags in deserialize_any
    # The deserialize_any function validates UTF-8 for !!binary tags, which fails for non-UTF-8 binary data
    # PNG magic bytes: \x89PNG\r\n\x1a\n (0x89 is not valid UTF-8)
    # Issue: https://github.com/bourumir-wyngs/serde-saphyr/blob/master/src/de.rs#L730
    # Will be fixed in future serde-saphyr version
    Given response predicates
      ```yaml
      - Body:
          contains: !!binary iVBORw0KGgo=
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/books/victim-prime/covers
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/books/victim-prime/covers
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

  @response @body @plain @contains @allow.failed
  Scenario: Body Contains - binary pattern with null bytes - UNSUPPORTED (serde-saphyr limitation)
    # FIXME: Currently unsupported due to serde-saphyr issue with !!binary tags in deserialize_any
    # Pattern contains null bytes (0x00) which are not valid UTF-8 in this context
    Given response predicates
      ```yaml
      - Body:
          contains: !!binary iVBORw0KGgo=
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/books/victim-prime/covers
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records

  @response @body @plain @contains @allow.failed
  Scenario: Body Contains - binary pattern spanning chunk boundaries - UNSUPPORTED (serde-saphyr limitation)
    # FIXME: Currently unsupported due to serde-saphyr issue with !!binary tags in deserialize_any
    # Tests streaming with 20-byte chunks where PNG magic bytes span chunk boundaries
    Given response predicates
      ```yaml
      - Body:
          contains: !!binary iVBORw0KGgo=
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/books/victim-prime/covers?streaming=true
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    When execute request
      ```hurl
      GET http://localhost/v1/books/victim-prime/covers?streaming=true
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "HIT"

