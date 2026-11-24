---
name: aicgen-patterns-base
description: "Value Object, Special Case (null object), Registry and Plugin patterns. Use when a small supporting object or lookup abstraction is needed."
---

# Base Patterns

## Rules

- Use a Value Object (immutable, equality by value) for concepts like money, email or date range instead of primitives.
- Use Special Case (null object) to avoid scattering null checks.
- Use Registry and Plugin sparingly: prefer explicit dependency injection to global lookups.
- Apply a pattern only when it removes real complexity; do not add it speculatively.

## Value Object

Immutable object defined by its value, not identity.

## Special Case (Null Object)

Replace null checks with polymorphism.

## Registry

Global access point for services.

## Plugin

Extend behavior without modifying core code.

## Best Practices

- Use Value Objects to avoid primitive obsession
- Make Value Objects immutable
- Use Special Case instead of null checks
- Prefer dependency injection over Registry
