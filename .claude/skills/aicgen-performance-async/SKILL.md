---
name: aicgen-performance-async
description: "Async performance: parallel execution, partial failures, batching, debouncing, throttling and concurrency limits. Use when many I/O operations are slow or overload a dependency."
---

# Async Performance Patterns

## Rules

- Run independent async operations in parallel (`Promise.all` or the language equivalent), not in sequence.
- Use settled/partial-result variants (`allSettled`) when one failure should not discard the other results.
- Cap concurrency (about 5-20) for bulk work so you do not overload a database or an API.
- Batch many small operations into bulk calls.
- Debounce input-driven work and throttle high-frequency events; always give async calls timeouts.
