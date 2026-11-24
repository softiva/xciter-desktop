---
name: aicgen-practices-refactoring
description: "Common code smells and safe refactoring steps. Use when cleaning up or restructuring existing code."
---

# Refactoring Patterns

## Rules

- Refactor only with passing tests; add characterization tests first if there are none.
- Change behavior or structure, never both in one commit; make small steps and run the tests after each.
- Target real smells: long methods, duplication, primitive obsession, feature envy; leave code that works and is rarely touched.
- Keep refactoring commits separate from feature work so they are easy to review and revert.

## Common Code Smells

### Long Method
Split into smaller, focused functions.

### Duplicate Code
Extract common logic.

### Primitive Obsession
Use value objects.

### Feature Envy
Move method to class it uses most.

## Safe Refactoring Steps

1. Ensure tests pass before refactoring
2. Make one small change at a time
3. Run tests after each change
4. Commit frequently
5. Refactor in separate commits from feature work

## Best Practices

- Refactor when adding features, not separately
- Keep refactoring commits separate
- Use IDE refactoring tools when available
- Write tests before refactoring if missing
