---
name: aicgen-api-pagination
description: "Offset, cursor and keyset pagination, response links and limits. Use when an endpoint returns a collection."
---

# API Pagination

## Rules

- Paginate every collection endpoint, with a default page size (about 20) and a hard maximum (about 100).
- Prefer cursor or keyset pagination for large or changing data; offset pagination degrades with depth and skips or repeats rows under writes.
- Order by a stable, unique key (for example `created_at, id`) and index it.
- Return the next cursor or links and whether more results exist; treat cursors as opaque.
- Reject invalid page or limit values with 400 instead of silently clamping unexpected input.

## Keyset Pagination

```sql
-- Most efficient for large tables
-- First page
SELECT * FROM products
ORDER BY created_at DESC, id DESC
LIMIT 20;

-- Next page (using last item's values)
SELECT * FROM products
WHERE (created_at, id) < ('2024-01-15 10:00:00', 12345)
ORDER BY created_at DESC, id DESC
LIMIT 20;
```
