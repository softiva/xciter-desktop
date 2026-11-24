---
name: aicgen-api-versioning
description: "API versioning strategies (path, header, query), deprecation and compatibility. Use when changing a public API in a breaking way."
---

# API Versioning

## Rules

- Version from day one, in the URL path (`/v1`) unless you have a strong reason for headers.
- Make additive changes (new optional fields, endpoints) without a new version; only breaking changes (removed or renamed fields, changed types or semantics) need one.
- Support the previous version for a published period and announce deprecations with `Deprecation`, `Sunset` and `Link` headers.
- Document migrations and keep contract tests for each supported version.

## Versioning Strategies

### URL Path Versioning
```
GET /api/v1/users
GET /api/v2/users
```

### Header Versioning
```
GET /api/users
Accept: application/vnd.api+json; version=2
```

### Query Parameter
```
GET /api/users?version=2
```

## Best Practices

- Version from the start
- Support at least N-1 versions
- Document deprecation timeline
- Provide migration guides
- Use semantic versioning for breaking changes
- Consider backwards-compatible changes first
