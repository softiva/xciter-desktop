---
name: aicgen-patterns-data-access
description: "Repository, Data Mapper, Unit of Work and Identity Map patterns. Use when designing persistence access."
---

# Data Access Patterns

## Rules

- Use the Repository pattern to give the domain a collection-like interface to persistence; keep query details out of business code.
- Use Data Mapper to keep domain objects ignorant of persistence; use Active Record only for simple CRUD.
- Use Unit of Work to commit related changes in one transaction.
- Use Identity Map to avoid loading the same entity twice per transaction.
- Do not expose ORM entities or query builders outside the data layer.

## Repository

Collection-like interface for domain objects.

## Data Mapper

Complete separation between domain and persistence.

## Unit of Work

Track changes and commit together.

## Identity Map

Ensure each object loaded only once per session.

## Best Practices

- Return domain objects from repositories
- Use one repository per aggregate root
- Keep repositories focused on persistence
- Don't leak database details into domain
