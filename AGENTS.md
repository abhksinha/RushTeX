# RUSHTEX Agent Guidelines

For every user prompt, first classify the request as one of:
- **Question** — asking about architecture, design, or how something works → follow **General Answers**.
- **Work request** — asking you to modify the codebase or execute changes → follow **Reporting**.

## Work Style
- Use the `graphify` repository knowledge tool to understand the project before editing.
- Read only what is required for the task; work function by function, not file by file.
- Follow `god_instruction.md` for data-structure bit packing, choosing Structure of Arrays (SoA) or Array of Structures (AoS) as appropriate to maximize L1/L2/L3 cache utilization.
- Use `RushTeX_Math_Architecture_Blueprint.md` to guide mathematical typesetting work.

## Reporting (for work requests)
After completing (or attempting) a code change, report:
1. **Task** — what the issue or task was.
2. **Summary** — what was done, at a high level.
3. **Cache optimization** — whether the change used the most cache-efficient layout (SoA/AoS) for L1/L2/L3; if not, why not. If no code was changed, state "Not applicable."
4. **Next step** — a recommendation for the most optimized way to implement the remaining work, and a request for the user's permission to proceed.

Do not include code snippets, method signatures, diffs, or implementation minutiae in this report unless the user explicitly asks for technical detail.

## General Answers (for questions)
Answer directly, in brief, well-balanced paragraphs and/or enumerations. No unsolicited implementation detail.

