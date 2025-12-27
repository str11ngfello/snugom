# RediSearch Escaping: Lessons Learned

This document captures the complexities of RediSearch query escaping, the bugs we encountered, and recommendations for maintaining and extending this system.

## Table of Contents
1. [Executive Summary](#executive-summary)
2. [RediSearch Field Types and Their Escaping Needs](#redisearch-field-types-and-their-escaping-needs)
3. [The Dual Role Problem: Tokenizers vs Query Operators](#the-dual-role-problem-tokenizers-vs-query-operators)
4. [Current Escaping Functions](#current-escaping-functions)
5. [Bugs We Encountered](#bugs-we-encountered)
6. [Lessons Learned](#lessons-learned)
7. [Current Filter Types Exposed to Users](#current-filter-types-exposed-to-users)
8. [Naming Critique and Recommendations](#naming-critique-and-recommendations)
9. [Future Considerations](#future-considerations)

---

## Executive Summary

RediSearch escaping is complex because the same characters serve different roles at different times:
- **At index time**: Characters like `-` and `/` act as **tokenizers**, splitting text into searchable terms
- **At query time**: The same characters act as **query operators** (e.g., `-` means "NOT")

This dual role means you can't simply "escape everything" - you must understand what happens at each stage and escape accordingly.

---

## RediSearch Field Types and Their Escaping Needs

### TAG Fields
- **Use case**: Exact matching of enumerated values (status, visibility, tags)
- **Index behavior**: Values are stored as-is, with optional separator for multi-value
- **Query syntax**: `@field:{value}` or `@field:{val1|val2}` for OR
- **Escaping needs**: Escape `\ { } [ ] : | " ' - . ` and spaces

### TEXT Fields
- **Use case**: Full-text search, partial matching, phrase search
- **Index behavior**: Values are **tokenized** on punctuation and whitespace
- **Query syntax**: `@field:term`, `@field:term*`, `@field:"phrase"`, `@field:%fuzzy%`
- **Escaping needs**: Complex - see below

### NUMERIC Fields
- **Use case**: Range queries, exact numeric matching
- **Query syntax**: `@field:[min max]`, `@field:[10 +inf]`
- **Escaping needs**: None (just format numbers correctly)

### GEO Fields
- **Use case**: Location-based queries
- **Query syntax**: `@field:[lon lat radius unit]`
- **Escaping needs**: None

---

## The Dual Role Problem: Tokenizers vs Query Operators

This is the core complexity we struggled with.

### At Index Time
When you store a TEXT field value like `cli-kv-tests/data/config`:
```
Stored value: "cli-kv-tests/data/config"
Tokenized into: ["cli", "kv", "tests", "data", "config"]
```

The `-` and `/` characters **split the text into tokens**. The tokens in the index do NOT contain these characters.

### At Query Time
When you query `@path:cli-kv-tests`:
```
Query: @path:cli-kv-tests
Parsed as: @path:cli NOT kv NOT tests
Result: EXCLUDES all documents containing "kv" or "tests"
```

The `-` character is interpreted as the **NOT operator**, completely changing the query semantics!

### The Solution
For TEXT field prefix queries, we must:
1. Tokenize the query value ourselves (split on `-` and `/`)
2. Build a multi-term query with the same tokens
3. Add wildcard only to the last token for prefix matching

```
Input: "cli-kv-tests/data"
Our tokenization: ["cli", "kv", "tests", "data"]
Generated query: @path:cli kv tests data*
```

This matches the tokenization that occurred at index time.

---

## Current Escaping Functions

### File: `snugom/src/search/mod.rs`

| Function | Purpose | Characters Escaped | Use When |
|----------|---------|-------------------|----------|
| `escape_for_tag_query(value)` | TAG field values | `$ { } \ \| - .` | Exact tag matching |
| `escape_text_for_wildcard(value)` | TEXT wildcard queries | `\ ( ) \| ' " [ ] { } : @ ? ~ & ! . * %` | Contains, fuzzy queries |
| `escape_text_for_phrase(value)` | TEXT phrase queries | `\ "` only | Value will be wrapped in quotes |
| `escape_text_token(token)` | Individual token escaping | `\ ( ) \| ' " [ ] { } : @ ? ~ & ! .` | After manual tokenization |
| `escape_text_search_term(token)` | Search term with wildcard | Calls `escape_text_token` + adds `*` | Free-text search terms |

**Important Update (2025)**: TAG field escaping was updated based on testing. Required escaping: `$ { } \ | - .`. Note that `-` (hyphen) must be escaped because it acts as the NOT operator in RediSearch query syntax, and `.` (period) must be escaped because it acts as a JSON path separator. Spaces, colons, brackets, and quotes are allowed without escaping.

### Critical Note: What's NOT Escaped

`escape_text_for_wildcard` and `escape_text_token` deliberately do NOT escape:
- `-` (hyphen) - Would break tokenization matching
- `/` (slash) - Would break tokenization matching
- `_` (underscore) - Tokenizer in some configurations

If you escape these, your query tokens won't match the indexed tokens!

---

## Bugs We Encountered

### Bug 1: TEXT Prefix Query Returns No Results

**Symptom**: Query for `path:prefix:cli-kv-tests/list` returned 0 results despite documents existing.

**Root Cause**: The `-` character in the query was being interpreted as NOT:
```
Query: @path:cli-kv-tests/list*
Parsed: @path:cli NOT kv NOT tests/list*
Effect: Excluded all documents containing "kv" or "tests"
```

**Fix**: Manual tokenization before building the query:
```rust
// Split on tokenizer characters ourselves
let tokens: Vec<&str> = value
    .split(|c| c == '-' || c == '/')
    .filter(|s| !s.is_empty())
    .collect();

// Build space-separated query
// Input: "cli-kv-tests/abc/list"
// Output: "cli kv tests abc list*"
```

### Bug 2: Escaping `/` Broke TEXT Queries

**Symptom**: After adding `\/` escaping, prefix queries stopped matching.

**Root Cause**: We escaped `/` in the query, but the indexed tokens don't contain `\/`. The query term `config\/db` doesn't match the indexed token `config` or `db`.

**Fix**: Remove `/` from the escaping list for TEXT fields.

### Bug 3: Initial `escape_text_token` Used Everywhere

**Symptom**: Every token in prefix queries had wildcards, causing overly broad matches.

**Root Cause**: `escape_text_token` adds `*` to every token, but we only want wildcard on the last token for prefix matching.

**Fix**: Create `escape_token_chars` (no wildcard) for use on all but the last token.

---

## Lessons Learned

### 1. Tokenization Happens Twice (Index Time AND Query Time)
You must understand both stages. A character that's a tokenizer at index time might be an operator at query time.

### 2. You Cannot "Escape Everything"
Blindly escaping all special characters breaks tokenization matching. The indexed tokens and query tokens must use the same tokenization rules.

### 3. TAG vs TEXT Fields Have Completely Different Escaping Rules
- TAG: Escape everything, value is stored and matched exactly
- TEXT: Only escape query operators, NOT tokenizers

### 4. RediSearch Documentation is Incomplete
The official docs list special characters but don't explain the tokenization interaction clearly. Empirical testing was required.

### 5. Test with Real Data Patterns
We only caught these bugs when testing with real path patterns like `cli-kv-tests/uuid/list/`. Simple test values like `config` didn't expose the issue.

### 6. Different Query Types Need Different Escaping

| Query Type | Escaping Strategy |
|------------|-------------------|
| TAG exact | Escape everything |
| TEXT prefix | Tokenize ourselves, escape tokens (not tokenizers), wildcard on last |
| TEXT contains | Escape operators only, preserve tokenizers |
| TEXT exact phrase | Escape only quotes and backslashes (wrapped in quotes) |
| TEXT fuzzy | Escape operators only, wrap in `%` |

---

## Current Filter Types Exposed to Users

### API Filter Syntax
```
?filter=field:operator:value
```

### Supported Operators

| Operator | Field Types | Query Generated | Example |
|----------|-------------|-----------------|---------|
| `eq` | TAG, TEXT* | TAG: `@field:{value}`, TEXT: prefix | `status:eq:active` |
| `range` | NUMERIC | `@field:[min max]` | `count:range:10,50` |
| `bool` | TAG (boolean) | `@field:{true\|false}` | `active:bool:true` |
| `prefix` | TEXT | `@field:token1 token2 last*` | `path:prefix:config/db` |
| `contains` | TEXT | `@field:*value*` | `desc:contains:error` |
| `exact` | TEXT | `@field:"phrase"` | `name:exact:John Doe` |
| `fuzzy` | TEXT | `@field:%value%` | `name:fuzzy:jonh` |

*Note: `eq` on TEXT fields defaults to prefix behavior for backwards compatibility.

---

## Function Names (Updated 2025)

The escaping functions have been renamed for clarity:

| Function | Purpose |
|----------|---------|
| `escape_for_tag_query` | Escape value for TAG field queries (escapes `$ { } \ \| - .`) |
| `escape_for_text_prefix` | Tokenize on `-`/`/`, escape tokens, wildcard last |
| `escape_for_text_contains` | Escape and wrap with `*value*` for contains queries |
| `escape_for_text_exact` | Escape and wrap with `"value"` for exact phrase queries |
| `escape_for_text_fuzzy` | Escape and wrap with `%value%` for fuzzy queries |
| `escape_for_text_search` | Escape a search term and add trailing wildcard |

---

## Future Considerations

### Potential Issues

1. **Underscore Tokenization**: By default, RediSearch may or may not tokenize on `_`. If we add underscore support, we need to update tokenization logic.

2. **Unicode Characters**: Current escaping focuses on ASCII. Unicode punctuation may have unexpected behavior.

3. **Query Injection**: If user input is passed directly to query building without proper escaping, RediSearch syntax could be injected. All escaping functions must be used correctly.

4. **Performance of Prefix Queries**: Very short prefix values (1-2 chars) can match many documents. Consider minimum length validation.

5. **Nested Field Paths**: JSON paths like `$.metadata.tags` require different escaping in the SCHEMA definition vs queries.

### Future Features to Consider

1. **Negative Filters**: Support `NOT` queries like `status:not:deleted`
   - Would need to wrap clause in `-(...)`

2. **Multi-value TAG OR**: Currently supported via `|` separator
   - Consider explicit syntax: `status:eq:active|pending`

3. **Range Exclusion**: Support `(min max)` for exclusive ranges
   - Currently all ranges are inclusive

4. **Regex Support**: RediSearch supports regex in TEXT fields
   - Would need heavy escaping: `@field:/pattern/`

5. **Geo Radius Queries**: Not currently exposed via filter syntax
   - Would need special handling: `location:geo:lat,lon,radius,unit`

6. **Highlighting/Snippets**: RediSearch can return highlighted matches
   - Would need different result parsing

### Documentation Debt

- Add inline examples to each escaping function
- Add "when to use" docstrings
- Create a decision flowchart for choosing the right escaping function
- Add integration tests with real RediSearch for each filter type

---

## Quick Reference: Escaping Decision Tree

```
What type of field?
├── TAG → use escape_for_tag_query()
│   (Escapes: $ { } \ | - .)
├── NUMERIC → no escaping needed
├── GEO → no escaping needed
└── TEXT → What type of query?
    ├── Prefix query?
    │   └── Use escape_for_text_prefix() - handles tokenization and wildcards
    ├── Contains query (*value*)?
    │   └── Use escape_for_text_contains() - wraps with *value*
    ├── Exact phrase ("value")?
    │   └── Use escape_for_text_exact() - wraps with "value"
    ├── Fuzzy (%value%)?
    │   └── Use escape_for_text_fuzzy() - wraps with %value%
    └── Free-text search (user input)?
        ├── Split on whitespace
        ├── Use escape_for_text_search() on each (adds *)
        └── Join with spaces
```

---

## References

- [RediSearch Query Syntax](https://redis.io/docs/stack/search/reference/query_syntax/)
- [RediSearch Escaping](https://redis.io/docs/stack/search/reference/escaping/)
- [RediSearch Tokenization](https://redis.io/docs/stack/search/reference/tokenization/)
