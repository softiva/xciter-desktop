---
name: aicgen-solid-principles
description: "SOLID principles with examples: SRP, OCP, LSP, ISP and DIP. Use when designing classes or reviewing coupling."
---

# SOLID Principles

## Rules

- Single responsibility: a class or module has one reason to change; split code that mixes persistence, business rules and I/O.
- Open/closed: add behavior through new implementations of an interface instead of editing branching `if/else` or `switch` chains.
- Liskov: a subtype must honor the base type's contract (no stricter preconditions, no surprise exceptions); if it cannot, do not inherit.
- Interface segregation: define small, client-focused interfaces rather than one wide one.
- Dependency inversion: depend on abstractions and inject dependencies through constructors; do not instantiate infrastructure inside business logic.
- Prefer composition over inheritance, and do not add abstractions until there is a second use or a real need to substitute.

## Single Responsibility Principle (SRP)

A class should have only one reason to change.

## Open/Closed Principle (OCP)

Open for extension, closed for modification.

## Liskov Substitution Principle (LSP)

Subtypes must be substitutable for their base types.

## Interface Segregation Principle (ISP)

Clients shouldn't depend on interfaces they don't use.

## Dependency Inversion Principle (DIP)

Depend on abstractions, not concretions.

## Best Practices

- Apply SRP at class, method, and module levels
- Use interfaces and dependency injection for flexibility
- Prefer composition over inheritance
- Design small, focused interfaces
- Inject dependencies rather than creating them internally
