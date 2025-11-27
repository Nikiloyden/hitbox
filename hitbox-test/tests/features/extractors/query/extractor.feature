Feature: Request Query Cache Key Extractor

  Background:
    Given hitbox with policy
      ```yaml
      Enabled:
        ttl: 10
      ```

  @extractor @query
  Scenario: Extract query parameter for cache key
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Query: "page"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      [Query]
      page: 1
      ```
    Then cache key exists
      ```
      page: "1"
      ```

  @extractor @query
  Scenario: Missing query parameter creates cache key without that part
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Method:
      - Query: "page"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      ```
    Then cache key exists
      ```
      method: "GET"
      ```

  @extractor @query
  Scenario: Multiple query parameters
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Query: "page"
      - Query: "limit"
      - Query: "sort"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      [Query]
      page: 2
      limit: 20
      sort: title
      ```
    Then cache key exists
      ```
      page: "2"
      limit: "20"
      sort: "title"
      ```

  @extractor @query @regex
  Scenario: Extract query parameter value with regex
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Query:
          name: filter
          value: "^status:(.+)$"
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      [Query]
      filter: status:active
      ```
    Then cache key exists
      ```
      filter: "active"
      ```

  @extractor @query @starts
  Scenario: Extract query parameters by prefix
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Query:
          name:
            starts: utm_
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      [Query]
      utm_source: google
      utm_medium: cpc
      other_param: ignored
      ```
    Then cache key exists
      ```
      utm_medium: "cpc"
      utm_source: "google"
      ```

  @extractor @query @explicit-eq
  Scenario: Extract query parameter with explicit eq operation
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Query:
          name:
            eq: session_id
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books
      [Query]
      session_id: abc123
      ```
    Then cache key exists
      ```
      session_id: "abc123"
      ```

  # Note: Array query parameters require bracket syntax (color[]=a&color[]=b)
  # The repeated key format (color=a&color=b) is not supported by serde_qs
  @extractor @query @array
  Scenario: Extract query parameter with array values
    Given request predicates
      ```yaml
      - Method: GET
      ```
    And key extractors
      ```yaml
      - Query: color
      ```
    When execute request
      ```hurl
      GET http://localhost/v1/authors/robert-sheckley/books?color[]=red&color[]=blue&color[]=green
      ```
    Then response status is 200
    And response header "X-Cache-Status" is "MISS"
    And cache has 1 records
    Then cache key exists
      | color | red   |
      | color | blue  |
      | color | green |
