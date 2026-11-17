# Linebender pull requests

This folder holds one file for each change that Runebender proposes to a Linebender project.
[OVERVIEW.md](OVERVIEW.md) lists all planned changes and the suggested order.
[TEMPLATE.md](TEMPLATE.md) is the form for a new file.

The process has one purpose: follow the [Linebender LLM policy](https://linebender.org/wiki/llm-policy/) exactly, with the least manual work for the owner.
Read the policy again before each pull request, because it can change.

## What the policy requires

- A pull request generated end to end by an LLM is not allowed.
- A pull request description written by an LLM is not allowed.
- LLM-written analysis in issues, review comments or Zulip is not allowed, even if a person checked it.
- Agent instruction files are not allowed in the pull request.
- LLM use is disclosed in the pull request description before a reviewer asks. This covers code an agent wrote, code pasted from a chat tool, generated tests, and code written by hand from a plan that an LLM proposed.
- Debugging, exploring code, running tests, and spelling or grammar checks of your own text need no disclosure.
- Do not state that a pull request has no LLM content.
- Review your own pull request with full effort before you ask anyone to read it.

## Who does what

The split below is what keeps the work inside the policy.
It is also what keeps the owner's part small: three short tasks for each pull request.

| Part | Agent | Owner |
| --- | --- | --- |
| Research: upstream code, history, existing issues and pull requests, maintainer statements | Does it, with links | Reads it |
| Design: what to change, and the options | Lists the options with facts | Decides, and writes the decision in one or two sentences |
| Code | May write a draft on a branch of the fork | Reads every line, changes what is not right, and can explain each line |
| Tests and upstream checks | Runs them and records the output | Reads the result |
| Title and description | Does not write or draft them. Corrects spelling and grammar only when asked. | Writes them, by typing or dictation |
| LLM disclosure | Keeps an exact record of what the agent did | Writes the disclosure sentence from that record |
| Opening the pull request | Does not open it | Runs the submit script, which asks for confirmation |
| Review comments and replies | Does not write them. Can explain code and run experiments. | Writes every reply |

The owner's three tasks are: decide the design, review the diff, and write the description.
Dictation counts as the owner's own writing.
An agent that receives dictated text corrects spelling and grammar and changes nothing else.

## The steps for one pull request

1. **Research.** The agent fills the "Upstream context" section of the file: the code at a named upstream commit, related issues and pull requests found with `gh`, earlier discussion, and the checks upstream runs. Each claim has a link or a file path. A claim that was not checked is marked "not checked".
2. **Decision.** The owner reads the research and writes the "Owner's decision" section. If the change adds public API, the owner asks upstream first, on Zulip or in an issue, in the owner's own words.
3. **Code.** The change is made on a branch of the fork that starts from current upstream `main`, one change for each branch. The "Code record" section lists each commit and who wrote it.
4. **Verification.** The agent runs the upstream checks and builds Runebender against the branch. The commands and results go in the "Verification" section.
5. **Owner review.** The owner reads the full diff and records the reviewed commit in the header of the file.
6. **Text.** The owner writes the title, the description and the disclosure in the "Pull request text" section.
7. **Submit.** The owner runs `scripts/upstream/submit_pull_request.sh <file>`. The script checks the approvals, shows the exact text, asks for typed confirmation, and then runs `gh pr create`.
8. **After.** The link goes in the "Record" section. Replies to reviewers are the owner's. When the pull request merges, update the pinned revision in `Cargo.toml` and delete the workaround.

## Rules for agents

- Do not write, draft, outline or reword the title, the description or the disclosure. The only permitted edit is a spelling or grammar correction that the owner asks for.
- Do not run `scripts/upstream/submit_pull_request.sh`, `gh pr create`, `gh pr comment`, `gh issue create` or `gh issue comment` for an upstream repository. Do not push to the fork unless the owner asks in the conversation.
- Do not set the approval fields in a file header. They belong to the owner.
- Keep the "Code record" exact. The disclosure depends on it.
- Do not copy text from the research sections into anything that goes upstream.
- Use `gh` for reading only: `gh search`, `gh issue view`, `gh pr view`, `gh pr diff`.

## How the script enforces this

`scripts/upstream/submit_pull_request.sh` refuses to continue unless all of these are true:

- it runs in an interactive terminal, so an agent's shell cannot run it;
- the header has `owner_wrote_text: yes` and `owner_decided_design: yes`;
- `owner_reviewed_commit` equals the current head commit of the branch, so a commit added after the review blocks the submission;
- the title, the description and the disclosure are present, and no template placeholder remains;
- the person at the terminal types the repository name to confirm.

The script opens a draft pull request by default.
The owner marks it ready on GitHub after one more look at the rendered page.
