---
name: Spec Plan
description: Runs a Kiro-style spec-driven planning workflow for a new feature or change. Generates requirements.md then design.md under .claude/scratch/<identifier>/, pausing for explicit user approval after each, then hands off to Claude Code's native plan mode (EnterPlanMode/ExitPlanMode) to produce the implementation plan instead of a separate tasks.md. Use when the user asks to "spec out", "spec plan", "write requirements", "write a design doc", or plan a feature before implementing it.
---

# Spec Plan

A three-phase spec-driven workflow: **Requirements → Design → Implementation Plan**. Each phase produces one artifact and requires explicit user approval before the next phase starts. The first two phases write markdown files to disk; the third uses Claude Code's built-in plan mode instead of generating a separate tasks list.

## Phase 0: Identifier and scratch dir

- Derive a short kebab-case `<identifier>` from the feature name (e.g. "Add dark mode toggle" → `dark-mode-toggle`). If the request is too vague to name, ask the user for a short name.
- Target directory: `.claude/scratch/<identifier>/` (already gitignored — do not commit these files unless the user explicitly asks).
- If that directory already exists with prior `requirements.md`/`design.md`, tell the user and ask whether to resume/revise the existing spec or start a new identifier.

## Phase 1: Requirements

1. If the request is ambiguous (unclear users, scope, constraints, out-of-scope items), ask clarifying questions before drafting. Don't over-ask — a couple of targeted questions beats a long interview.
2. Write `.claude/scratch/<identifier>/requirements.md` using this structure:

   ```markdown
   # Requirements: <Feature Name>

   ## Introduction

   <1-2 paragraph summary of the feature and why it's needed.>

   ## Requirements

   ### Requirement 1: <short name>

   **User Story:** As a <role>, I want <capability>, so that <benefit>.

   #### Acceptance Criteria

   1. WHEN <event/trigger> THEN <system> SHALL <expected response>
   2. IF <precondition> THEN <system> SHALL <expected response>
   3. WHEN <event> AND <condition> THEN <system> SHALL <expected response>

   ### Requirement 2: <short name>

   ...
   ```

   - Use the EARS pattern (WHEN/IF/THEN ... SHALL) for every acceptance criterion — it's testable and unambiguous.
   - Cover the happy path, edge cases, and error conditions as separate criteria or separate requirements.
   - List explicit **Out of Scope** items at the end if relevant.

3. Present the requirements to the user (show the file content or a tight summary) and ask directly: _"Do you approve these requirements, or would you like changes?"_
4. Iterate on feedback, rewriting the file, until the user gives explicit approval (e.g. "approved", "looks good", "yes"). Do not proceed to Phase 2 on an ambiguous or implied approval.

## Phase 2: Design

Only start once requirements are explicitly approved.

1. Explore the relevant parts of the codebase so the design is grounded in what actually exists — real file paths, real patterns, real conventions from the nested `CLAUDE.md` files. If `graphify-out/graph.json` exists, start there: `graphify query "<feature area>"` for broad context, `graphify explain "<concept>"` for a single entity/module, `graphify path "<A>" "<B>"` to see how two pieces connect. Fall back to Glob/Grep/Read (or the Explore agent for broad surveys) for anything the graph doesn't surface or when no graph exists.
2. Write `.claude/scratch/<identifier>/design.md` using this structure:

   ```markdown
   # Design: <Feature Name>

   ## Overview

   <How this satisfies the requirements, at a high level.>

   ## Architecture

   <Where this fits in the existing system. Include a mermaid diagram if it clarifies flow/components.>

   ## Components and Interfaces

   <Key files/modules to add or change, their responsibilities, and how they interact.>

   ## Data Models

   <New/changed types, schemas, or API contracts.>

   ## Error Handling

   <Failure modes and how each is handled.>

   ## Testing Strategy

   <What gets unit tested and how.>
   ```

   - Reference the specific requirement numbers this design satisfies so traceability is clear.
   - Keep it concrete — actual file paths under this repo, not generic placeholders.
   - **If the design touches a backend handler/DB request path** (a Lambda `handler`, an action in `backend/lambda/src/{entity}/`, or a `*-dsql.ts` query module), the Testing Strategy section must explicitly state whether integration coverage applies — via the harness added in MAVI-90 (`backend/lambda/test/integration/`, `*.integration.spec.ts`, `npm run test:integrations` against the dedicated `qa` company) — and why or why not. Don't leave it implicit as "unit tests only" without addressing the question.
   - **If the design includes a user-facing UI** (new screens, a materially changed existing screen, or a new interactive flow), invoke the `design-wireframe` skill (interactive mode, unless this run is itself autonomous — see that skill for the autonomous variant) to produce a low-fidelity wireframe before finalizing this phase. Reference the resulting file path/published URL in this design's Overview or Components section so it's discoverable later. Backend-only/internal-technical designs with no UI surface skip this.

3. Present the design (and, if one was produced, the wireframe) to the user and ask directly: _"Do you approve this design, or would you like changes?"_ Treat wireframe approval and design.md approval as one combined checkpoint — don't finalize Phase 2 with an approved design.md sitting alongside a still-unapproved wireframe.
4. Iterate until explicit approval of both. If approval surfaces a gap in the requirements themselves, go back and patch `requirements.md` first, note the change to the user, then continue.

## Phase 3: Implementation Plan (native plan mode)

Only start once the design is explicitly approved. This phase replaces Kiro's `tasks.md` with Claude Code's own plan mode — no third scratch file is generated.

1. Call `EnterPlanMode`.
2. In plan mode, read back the approved `requirements.md` and `design.md` and translate the design into a concrete, ordered implementation plan — the exact files to create/edit, in sequence, tied back to requirement numbers. Do any additional codebase exploration plan mode affords.
3. Write the plan to the plan file plan mode gives you, then call `ExitPlanMode` to request the user's approval.
4. Once approved, implementation proceeds as normal — this is the same plan-mode approval flow used for any other task, just seeded by the requirements/design docs instead of derived fresh.

## Rules

- Never skip a phase or infer approval — each of Phase 1 and Phase 2 ends with an explicit yes/no checkpoint before moving on.
- Don't generate a `tasks.md` — Phase 3 is Claude Code's native plan mode, not a written artifact in `.claude/scratch/`.
- If the user asks to jump straight to design or implementation without requirements, ask them to confirm they want to skip that checkpoint before doing so.
- Keep `requirements.md` and `design.md` in sync — a design change that contradicts a requirement means the requirement gets updated first.
- Phase 2 is not complete for a UI-touching design until its wireframe (via `design-wireframe`) is approved alongside `design.md` — don't treat a design with a still-open wireframe as ready for Phase 3.
