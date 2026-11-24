---
name: aicgen-patterns-gof
description: "Gang of Four creational, structural and behavioural patterns. Use when a classic design pattern may fit the problem."
---

# Gang of Four Patterns

## Rules

- Use a pattern to solve a concrete design problem, never because it exists; prefer simple code first.
- Prefer composition and interfaces over inheritance; Strategy and Decorator replace many subclass hierarchies.
- Creational: Factory Method or Builder for complex construction; avoid Singletons (use dependency injection instead).
- Behavioral: Strategy for interchangeable algorithms, Observer for event notification, Command for undoable or queued actions.
- Many patterns are language features in modern languages (closures, enums, iterators); use the built-in form.

## Best Practices

- Use patterns to solve specific problems, not everywhere
- Combine patterns when appropriate
- Favor composition over inheritance
- Keep implementations simple
