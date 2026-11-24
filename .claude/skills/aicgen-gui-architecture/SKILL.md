---
name: aicgen-gui-architecture
description: "MVC, MVP, MVVM and component architecture for user interfaces. Use when choosing how to structure UI code."
---

# GUI Architecture Patterns

## Rules

- Separate presentation from business logic with MVC, MVP or MVVM; do not put business rules or I/O in views.
- In MVVM the view model exposes observable state and commands, and the view binds to it; the view model has no reference to the view.
- Keep views passive and as free of logic as possible so logic stays unit-testable.
- Prefer small, composable components with explicit inputs and outputs over large screens with shared mutable state.
- Choose one pattern per application and apply it consistently.

## Best Practices

- Separate UI logic from business logic
- Keep views as simple as possible
- Use unidirectional data flow when possible
- Make components reusable and testable
- Choose pattern based on framework and team familiarity
