---
name: aicgen-api-rest
description: "REST design: resource-oriented URLs, methods, status codes, response format, nesting and versioning. Use when designing or reviewing a REST API."
---

# REST API Design

## Rules

- Model resources, not actions: plural nouns, lowercase hyphenated paths, at most two or three levels of nesting.
- Version from the start (`/api/v1/...`) and never break an existing version.
- Keep GET, PUT and DELETE idempotent; make POST retry-safe with an idempotency key where duplicates cost money.
- Wrap responses consistently (`data`, `meta`, `errors`) and use ISO 8601 UTC timestamps.
- Authenticate every non-public endpoint and authorize per resource, not per route.
- Paginate every collection and cap the page size.

## Resource-Oriented URLs

```
✅ Good (nouns, plural)
GET    /api/v1/books           # List books
GET    /api/v1/books/123       # Get book
POST   /api/v1/books           # Create book
PUT    /api/v1/books/123       # Replace book
PATCH  /api/v1/books/123       # Update book
DELETE /api/v1/books/123       # Delete book

❌ Bad (verbs, actions)
POST /api/v1/createBook
GET  /api/v1/getBookById/123
POST /api/v1/updateBook/123
```

## Hierarchical Resources

```
✅ Limit nesting to 2-3 levels
GET /api/v1/authors/456/books       # Books by author
GET /api/v1/orders/789/items        # Items in order

❌ Too deep
GET /api/v1/publishers/1/authors/2/books/3/reviews/4

✅ Use query parameters instead
GET /api/v1/reviews?bookId=3
```

## API Versioning

Version from the start (`/api/v1/...`) and never break an existing version. Strategies, deprecation headers and migration are covered by the API versioning guideline.
