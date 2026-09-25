---
name: Git Commit
description:
  A skill for generating and pushing git commits with standardized messages.
---

# Git Commit Skill

This skill provides a standardized workflow for committing changes to the git
repository. Use this skill whenever you are ready to save your progress or
complete a task.

## Workflow

1. **Add Changes**: Run `git add .` to stage all changes.
2. **Analyze Diffs**: Run `git diff` (and `git diff --cached` if changes are
   already staged) to understand exactly what has changed in the codebase.
3. **Summarize Changes**: Identify the key components or features that were
   affected and what the core impact of the change is.
4. **Construct Commit Message**:
   - **Short Message**: After the type and optional scope, provide a short,
     imperative-tense summary of the change, it should be a non-technical
     summary of the changes as it will be used to generate release notes.
   - **Brief Description**: Include a bulleted list of descriptions that explain
     the specific technical changes made.
5. **Commit Changes**: Run `git commit -m "<commit message>"` to commit the
   changes.
6. **Push Changes**: Confirm with the user before running `git push` to push the
   changes to the remote repository.

## Format

`<type>[optional scope]: <description>`

## Allowed Types

- **feat**: A new feature
- **fix**: A bug fix
- **docs**: Documentation only changes
- **style**: Changes that do not affect the meaning of the code (white-space,
  formatting, etc)
- **refactor**: A code change that neither fixes a bug nor adds a feature
- **perf**: A code change that improves performance
- **test**: Adding missing tests or correcting existing tests
- **chore**: Changes to the build process or auxiliary tools and libraries such
  as documentation generation

## Example Commit Message

```text
feat: implement invoice status transitions

- added transition logic for moving invoices from PENDING to PAID
- implemented validation for invalid status changes
- updated database schema to include updatedAt timestamp
```

---

```text
feat(auth): implement login with google

- added google login
```

---

## Prompting the Agent

When this skill is invoked, the agent should:

1. Review staged and unstaged changes.
2. Generate a commit message based on the rules above.
3. Present the message to the user or execute the commit if authorized.
