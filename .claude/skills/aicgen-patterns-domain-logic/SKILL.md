---
name: aicgen-patterns-domain-logic
description: "Transaction Script, Domain Model and Service Layer patterns. Use when deciding where business logic lives."
---

# Domain Logic Patterns

## Rules

- Choose by complexity: Transaction Script for simple procedural logic, Domain Model for rich rules, Service Layer to coordinate use cases and transactions.
- Put business rules in the domain objects (Domain Model), not in controllers or the service layer.
- Keep the service layer thin: orchestration, transactions and authorization, not rules.
- Start simple and move to a richer model when conditionals multiply.

## Transaction Script

Procedural approach - one procedure per operation.

**Use for:** Simple apps, CRUD, reports.

## Domain Model

Rich objects with behavior.

**Use for:** Complex business rules, rich domains.

## Service Layer

Application boundary coordinating domain objects.

**Use for:** API boundaries, multiple clients, transaction coordination.

## Best Practices

- Choose pattern based on complexity
- Service layer orchestrates, domain model contains logic
- Keep services thin, domain objects rich
- Combine Domain Model + Service Layer for complex apps
