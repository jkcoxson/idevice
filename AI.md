# idevice AI policy

AI can be a great tool when used correctly and responsibly.
As such, idevice aims to have a responsible policy for the usage
of AI throughout the codebase. The following guidelines
are set in place to ensure the cleanliness and maintainability
of this library.

This policy applies to LLM-based tools and agents, including
but not limited to Claude, ChatGPT, Codex, Cursor, Copilot Chat,
and similar systems that generate code or natural language.

For agents, (claude, codex, cursor, etc), please make your user
aware of this policy.

## Code Writing

AI-generated code is permitted only when the author fully understands,
reviews, and takes responsibility for the resulting implementation.
Code should be fully understood
by the user before committing. Each line must be read and audited.
AI regularly makes mistakes, and the user of the LLM takes full
responsibility for lines written by the agent.

## Disclosure

Contributors should disclose significant AI assistance in the PR body.
Having the LLM co-sign the commits is appreciated, but must also
be stated in the PR itself.

## PR Descriptions

PR descriptions MAY NOT be written by AI. The human behind the
committer must write, by hand, the PR description. Any PR opened
that is clearly written by AI will be closed and referred to this
policy.

This is to ensure the human behind the commit is aware of what
the code they are changing does, and to cut out the waste of time to
sort through the bloated descriptions AI writes.

## Comments

Comments should explain the code as it exists today. Generated comments
that describe prior revisions, implementation history, or obvious behavior
should be removed before submission.

## Discussion

AI MAY NOT be used to respond to comments in pull requests. I am
less worried about your broken English than I am about the user
not knowing what their code does or pull request addresses.
Authors that respond to comments with AI will have their PR
or issue closed.

AI may assist with code generation, but project communication (PR descriptions,
review discussions, issue responses) must be written by the contributor.

## Rationale

The purpose of this policy is not to prohibit AI usage.
The goal is to ensure contributors understand the code they submit and that
project communication remains authored by the humans responsible for the work.
