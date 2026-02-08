# Development Documentation

Comprehensive guides for contributing to Codanna.

## Language Implementation

Complete documentation for adding new language support:

### Core Documents

1. **[language-architecture.md](./language-architecture.md)** - System Design Principles
   - Design patterns and architecture decisions
   - State management strategy
   - Resolution and data flow
   - Performance patterns and trade-offs
   - Extensibility points
   - Read this to understand **WHY** the architecture works this way

2. **[language-support.md](./language-support.md)** - API Reference & Quick Start
   - Trait API documentation
   - File structure and organization
   - Implementation workflow
   - Registration and testing
   - Read this to learn **WHAT** you need to implement

3. **[language-patterns.md](./language-patterns.md)** - Implementation Patterns & Best Practices
   - Internal method organization (68 parser methods, 30+ behavior methods)
   - Naming conventions reference
   - Common patterns and heuristics
   - Step-by-step implementation guide
   - Read this to see **HOW** to implement with consistent patterns

### Reading Path

**For new contributors:**
1. Start with [language-architecture.md](./language-architecture.md) - understand the system
2. Review [language-support.md](./language-support.md) - learn the API contracts
3. Follow [language-patterns.md](./language-patterns.md) - implement with proven patterns

**For experienced developers:**
1. Quick scan [language-support.md](./language-support.md) - API reference
2. Copy patterns from [language-patterns.md](./language-patterns.md) - implementation examples
3. Refer to [language-architecture.md](./language-architecture.md) - edge cases and design decisions

**For reviewers:**
1. Check [language-architecture.md](./language-architecture.md) - verify design principles
2. Validate against [language-patterns.md](./language-patterns.md) - ensure consistency
3. Confirm [language-support.md](./language-support.md) - API compliance

### Quick Reference

| Need to... | Read |
|------------|------|
| Understand the 6-file structure | [language-architecture.md § Separation of Concerns](./language-architecture.md) |
| Know what methods to implement | [language-support.md § File Structure Reference](./language-support.md) |
| See how to implement parsing logic | [language-patterns.md § Parser Implementation](./language-patterns.md) |
| Understand resolution flow | [language-architecture.md § Resolution Architecture](./language-architecture.md) |
| Learn naming conventions | [language-patterns.md § Naming Conventions](./language-patterns.md) |
| Compare design alternatives | [language-architecture.md § Comparisons](./language-architecture.md) |
| Get implementation checklist | [language-support.md § Implementation Checklist](./language-support.md) |
| Find common patterns | [language-patterns.md § Common Patterns](./language-patterns.md) |

---

## Other Development Guides

- **[guidelines.md](./guidelines.md)** - General Rust coding principles and development rules
- **[parsers_api.md](./parsers_api.md)** - Parser API reference (legacy, see language-support.md)
- **[ci-local-remote-parity.md](./ci-local-remote-parity.md)** - CI/CD setup and testing
- **[tree-sitter-cli-setup.md](./tree-sitter-cli-setup.md)** - Tree-sitter CLI installation (legacy, see [../tree-sitter/README.md](../tree-sitter/README.md))

---

## Related Documentation

- **Tree-sitter tools**: [../tree-sitter/README.md](../tree-sitter/README.md)
- **Test infrastructure**: [../../tests/CLAUDE.md](../../tests/CLAUDE.md)
- **Project guidelines**: [../../CLAUDE.md](../../CLAUDE.md)
