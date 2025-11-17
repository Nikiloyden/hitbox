# Test Coverage Checklist for Response Predicates

## Response Predicates

### Response Status Predicate

#### Operations

##### Eq (Exact Match)
- [x] Exact status code match cached
- [x] Status code mismatch not cached
- [x] Multiple status predicates - all must match

##### In (Multiple Status Codes)
- [x] Status code in list cached
- [x] Status code not in list not cached
- [x] Empty status list behavior
- [x] Single status code in list
- [x] Multiple status codes in list (2-4 codes)

##### Range (Status Code Range)
- [x] Status in range cached
- [x] Status outside range not cached
- [x] Range lower boundary (inclusive)
- [x] Range upper boundary (inclusive)
- [x] Single-value range (start equals end)
- [x] Invalid range (start > end) validation (fails at configuration parse time)

##### Class (HTTP Status Class)
- [x] Success class (2xx) cached
- [x] Redirect class (3xx) behavior
- [x] Client error class (4xx) cached
- [x] Server error class (5xx) behavior
- [x] Informational class (1xx) behavior

#### Explicit Syntax Support
- [x] Eq operation with explicit syntax `{ eq: value }`
- [x] In operation with explicit syntax `{ in: [values] }`
- [x] Range operation with explicit syntax `{ range: [start, end] }` (required - no implicit form)
- [x] Class operation with explicit syntax `{ class: ClassName }`

#### Notes
- **Range** operation requires explicit syntax only: `{ range: [start, end] }`
- **In**, **Eq**, and **Class** support both explicit and implicit syntax
- Implicit array syntax `[values]` always resolves to **In** operation (not Range)
- **Range validation**: Invalid ranges (start > end) are rejected at configuration parse time with clear error messages
- Unit tests for range validation are in `hitbox-configuration/tests/test_reponse.rs`

### Response Header Predicate

#### Operations

##### Eq (Exact Match)
- [x] Exact header value match cached
- [x] Different header value not cached
- [x] Case-insensitive header name
- [x] Multiple header predicates - all must match
- [x] Header with empty value
- [x] Multiple header values with EQ operation
- [x] Header value case sensitivity
- [x] Header with whitespace trimmed
- [x] Header missing not cached

##### Exist (Presence Check)
- [x] Header exists cached
- [x] Header missing not cached
- [x] Case-insensitive header name in Exist

##### In (Multiple Values)
- [x] Value in list cached
- [x] Value not in list not cached
- [x] Multiple header values with IN operation
- [x] Single value in list
- [x] Empty list behavior
- [x] Header value case sensitivity with IN operation
- [x] Header missing not cached

##### Contains (Substring Match)
- [x] Header value contains substring cached
- [x] Header value doesn't contain substring not cached
- [x] Case-sensitive substring matching
- [x] Multiple headers with contains
- [x] Header missing not cached

##### Regex (Pattern Match)
- [x] Header value matches regex pattern cached
- [x] Header value doesn't match pattern not cached
- [x] Complex regex patterns
- [x] Multiple headers with regex
- [x] Header missing not cached
- [x] Case-sensitive regex matching
- [x] Invalid regex pattern handled at configuration parse time (unit test: `test_invalid_regex_pattern_rejected`)

#### Notes
- **Invalid regex validation**: Similar to range validation for status predicates, invalid regex patterns are tested at the unit test level in `hitbox-configuration/tests/test_reponse.rs`
- **Unit test coverage**: Contains, Regex deserialization, and validation tests added (`test_response_header_contains_deserialize`, `test_response_header_regex_deserialize`, `test_invalid_regex_pattern_rejected`, `test_valid_regex_pattern_accepted`)
- **BDD test coverage**: 32 scenarios covering all runtime behavior for Eq, Exist, In, Contains, and Regex operations

### Response Body Predicate

#### Plain Operations (Byte-based)

##### Contains (Substring Search)
- [x] Body contains text cached
- [x] Body doesn't contain text not cached
- [x] Empty pattern matches any body
- [x] Pattern at beginning of body
- [x] Pattern at end of body
- [x] Pattern in middle of body
- [x] Pattern spanning chunk boundaries (streaming)
- [x] Case-sensitive matching
- [x] Binary data patterns (3 scenarios implemented but skipped with @allow.failed due to serde-saphyr limitation)
  - [x] PNG magic bytes (binary data)
  - [x] Binary pattern with null bytes
  - [x] Binary pattern spanning chunk boundaries
- [x] Pattern with special characters
- [x] Multiple contains predicates - all must match
- [x] Multiple patterns, one doesn't match - response not cached

##### Starts (Prefix Match)
- [x] Body starts with prefix cached
- [x] Body doesn't start with prefix not cached
- [x] Empty prefix matches any body
- [x] Prefix longer than body
- [x] Long prefix match (most of body) cached
- [x] Streaming body with prefix check
- [x] Case-sensitive matching
- [x] Multiple starts predicates - all must match
- [x] Multiple predicates, one doesn't match - response not cached

##### Ends (Suffix Match)
- [x] Body ends with suffix cached
- [x] Body doesn't end with suffix not cached
- [x] Empty suffix matches any body
- [x] Suffix longer than body
- [x] Long suffix match (most of body) cached
- [x] Streaming body with suffix check
- [x] Case-sensitive matching
- [x] Multiple ends predicates - all must match
- [x] Multiple predicates, one doesn't match - response not cached

##### Eq (Exact Match)
- [x] Exact body match cached
- [x] Different body not cached
- [x] Streaming body collection
- [x] Case-sensitive matching
- [x] Whitespace sensitive
- [x] Partial match not sufficient
- [x] Multiple eq predicates - all must match
- [x] Multiple predicates, one doesn't match - response not cached

##### Regex (Pattern Match)
- [x] Simple pattern match cached
- [x] Pattern doesn't match not cached
- [x] Wildcard pattern (.*) cached
- [x] Character class pattern ([a-z]+) cached
- [x] Optional pattern (\s?) cached
- [x] Anchored pattern start (^) cached
- [x] Anchored pattern end ($) cached
- [x] Alternation pattern (|) cached
- [x] Case-sensitive matching
- [x] Multiline pattern cached
- [x] Digit pattern (\d{4}) cached
- [x] Streaming body collection
- [x] Multiple regex predicates - all must match
- [x] Multiple predicates, one doesn't match - response not cached

##### Limit (Size Check)
- [x] Body within size limit cached
- [x] Body exceeds size limit not cached
- [x] Body exactly at limit cached
- [x] Body one byte over limit not cached
- [x] Very large limit cached
- [x] Zero limit (only empty body) not cached
- [x] Streaming body with size hint cached
- [x] Streaming body exceeds limit not cached
- [x] Multiple limit predicates - all must match
- [x] Multiple predicates, one limit exceeded - response not cached
- [x] Combined with other predicates - both must match

#### Jq Operations (JSON-based)

##### Jq + Eq (Field Equality)
- [x] JSON field equals value cached
- [x] JSON field not equals value not cached
- [x] Nested field extraction (`.metadata.year`)
- [x] Array index access (`.[0].author`)
- [x] Array index access - value mismatch not cached
- [x] String field comparison
- [x] Number field comparison (array length)
- [x] Number field comparison - value mismatch not cached
- [x] Multiple jq predicates - all must match
- [x] Multiple predicates, one doesn't match - response not cached

##### Jq + Expression (Boolean Evaluation)
- [x] Array length check (`length == 3`) - response cached
- [x] Array length mismatch - response not cached
- [x] Count items matching condition - response cached
- [x] Check first item by id - response cached
- [x] Complex boolean logic with multiple conditions - response cached
- [x] Find value in array with any() - response cached
- [x] Find value in array with any() - no match - response not cached

##### Jq + Exist (Field Presence)
- [x] Field exists cached
- [x] Field missing not cached
- [x] Null field counts as existing
- [x] Nested field existence
- [x] Nested field missing not cached
- [x] Array element existence
- [x] Array element missing not cached
- [x] Field in object without metadata not cached
- [x] Multiple exist predicates - all must match
- [x] Multiple predicates, one field missing - response not cached

##### Jq + In (Field Value in List)
- [x] Field value in list cached
- [x] Field value not in list not cached
- [x] Empty list (never cached)
- [x] Single value list
- [x] Number value in list cached
- [x] Number value not in list not cached
- [x] Mixed value types in list
- [x] Missing field not cached
- [x] Nested field value in list cached
- [x] Array element value in list cached
- [x] Multiple in predicates - all must match
- [x] Multiple predicates, one not in list - response not cached

##### Jq Complex Expressions
- [x] Array filtering with any() (`any(.[]; .id == "value")`)
- [x] Array filtering with select (`[.[] | select(.author == "robert-sheckley")] | length >= 3`)
- [x] Complex boolean logic (multiple conditions with `and`)
- [x] Array mapping with eq (`.[].id` compared to array)
- [x] Array mapping with exist (check if mapping produces results)
- [x] Array mapping with in (check if mapped array in list)

#### Configuration Format

##### Plain Operations (Implicit Syntax)
```yaml
- Body:
    contains: "text"
- Body:
    starts: "prefix"
- Body:
    ends: "suffix"
- Body:
    eq: "exact match"
- Body:
    regex: "pattern.*"
- Body:
    limit: 1024
```

##### Jq Operations

```yaml
# Explicit syntax - extract field and compare
- Body:
    jq:
      expression: ".field"
      eq: "value"
- Body:
    jq:
      expression: ".status"
      exist: true
- Body:
    jq:
      expression: ".type"
      in: ["user", "admin"]

# Explicit syntax - boolean expression with eq: true
- Body:
    jq:
      expression: "length == 3"
      eq: true

# Implicit syntax - shorthand for boolean expressions
# Equivalent to { expression: "...", eq: true }
- Body:
    jq: "length == 3"
- Body:
    jq: 'any(.[]; .id == "value")'
```

#### Notes
- **Plain operations** work on raw bytes (UTF-8 text or binary data)
- **Jq operations** parse body as JSON and apply jq expression
- **JqExpression**: Renamed from JqFilter to better reflect that it evaluates jq expressions
- **Jq Configuration**: Supports both explicit and implicit syntax
  - Explicit with field extraction: `{ expression: ".field", eq: "value" }` - extracts field and compares value
  - Explicit with boolean: `{ expression: "length == 3", eq: true }` - evaluates expression and checks result is true
  - Implicit (shorthand): `"length == 3"` - equivalent to `{ expression: "length == 3", eq: true }`
- **Streaming support**: Contains, Starts use optimized streaming (don't buffer full body)
- **Full buffering**: Ends, Eq, Regex, Limit, all Jq operations collect full body
- **Error handling**: JSON parse errors, body read errors → non-cacheable
- **Unit test coverage**: 10 tests in `hitbox-http/tests/predicates/reponse/body.rs` covering Jq operations
- **Configuration tests**: Deserialization and validation in `hitbox-configuration`
- **BDD test coverage**:
  - **Plain operations** (byte-based, in `tests/features/predicates/response/body/plain/`):
    - ✅ Contains operation: 14 scenarios in `plain/contains.feature`
      - 11 passing scenarios (text patterns, special characters, streaming, multiple predicates)
      - 3 scenarios marked `@allow.failed` and skipped (binary data with `!!binary` tags - currently unsupported)
    - ✅ Starts operation: 9 scenarios in `plain/starts.feature`
      - All 9 scenarios passing (prefix matching, empty prefix, case sensitivity, multiple predicates)
    - ✅ Ends operation: 9 scenarios in `plain/ends.feature`
      - All 9 scenarios passing (suffix matching, empty suffix, case sensitivity, multiple predicates, streaming)
    - ✅ Eq operation: 8 scenarios in `plain/eq.feature`
      - All 8 scenarios passing (exact match, streaming, case sensitivity, whitespace sensitivity, multiple predicates)
    - ✅ Regex operation: 14 scenarios in `plain/regex.feature`
      - All 14 scenarios passing (wildcards, character classes, anchors, alternation, quantifiers, streaming, multiple predicates)
    - ✅ Limit operation: 11 scenarios in `plain/limit.feature`
      - All 11 scenarios passing (within/exceeds limit, exact boundaries, streaming, size optimization, multiple predicates)
  - **Jq operations** (JSON-based, in `tests/features/predicates/response/body/jq/`):
    - ✅ Jq + Eq operation (explicit): 10 scenarios in `jq/eq.feature`
      - All 10 scenarios passing (field equality, nested fields, array access, number/string comparison, multiple predicates)
    - ✅ Jq + Expression operation (explicit): 7 scenarios in `jq/eq.feature`
      - All 7 scenarios passing (boolean expressions, array length checks, array filtering with any()/select, complex boolean logic, item lookup by index)
    - ✅ Jq + Exist operation: 10 scenarios in `jq/exist.feature`
      - All 10 scenarios passing (field presence, null fields, nested fields, array elements, multiple predicates)
    - ✅ Jq + In operation: 12 scenarios in `jq/in.feature`
      - All 12 scenarios passing (value lists, empty/single/multiple values, mixed types, nested fields, array elements, multiple predicates)
    - ✅ Jq complex expressions: 5 scenarios in `jq/complex.feature`
      - All 5 scenarios passing (array mapping with eq/exist/in operations)
    - ✅ Jq implicit syntax: 4 scenarios in `jq/implicit.feature`
      - All 4 scenarios passing (simple boolean, any() search, complex logic, negative cases)
  - **Binary data limitation**: YAML `!!binary` tags with non-UTF-8 data are currently unsupported due to serde-saphyr limitation
    - Root cause: `deserialize_any` validates UTF-8 for `!!binary` tags ([de.rs#L730](https://github.com/bourumir-wyngs/serde-saphyr/blob/master/src/de.rs#L730))
    - Custom deserializer works correctly for direct deserialization but fails through untagged enums
    - Scenarios with `@allow.failed` tag are skipped by default (see `tests/integration/bdd.rs`)
    - Will be fixed in future serde-saphyr version
  - Total: 113/113 scenarios implemented (100%), 110 passing, 3 skipped
