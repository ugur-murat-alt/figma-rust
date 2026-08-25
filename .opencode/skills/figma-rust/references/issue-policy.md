# Mandatory GitHub issue policy

Repository: `ugur-murat-alt/figma-rust`

This procedure implements the standing decision in `SKILL.md`: each independently reproducible non-security root cause or coherent actionable improvement discovered during system use must produce one GitHub issue before the task is reported complete. A duplicate is linked instead of recreated.

## Decision sequence

1. **Classify the observation.** Choose exactly one: expected/documented behavior, local setup error, reproducible product defect, actionable improvement, security-sensitive report, or insufficient evidence.
2. **Apply the public-safety filter.** If the report could expose a vulnerability, credential, private Figma file, customer data, proprietary screenshot, access token, local path containing private identity, or exploit details, do not create a public issue. Stop and request a private reporting path.
3. **Reproduce or bound it.** Bugs require the smallest deterministic reproduction. Improvements require a concrete problem, affected layer, proposed outcome, and observable acceptance criteria.
4. **Check existing documentation.** Read `docs/plan.md`, `docs/figma-gpui-capability-matrix.md`, relevant fixture README files, and existing diagnostics. A known limitation needs new evidence or a new bounded proposal.
5. **Search duplicates.** Run a title/keyword search, then a second search using the diagnostic code when one exists. If there is no diagnostic code, use the affected capability, symbol, fixture, or command as the second search:

   ```sh
   gh issue list --repo ugur-murat-alt/figma-rust --state all --search 'KEYWORDS'
   gh issue list --repo ugur-murat-alt/figma-rust --state all --search 'DIAGNOSTIC_CODE_OR_CAPABILITY'
   ```

   If a matching issue exists, do not create another. Add new evidence only when the task explicitly authorizes commenting; otherwise report the existing URL.
6. **Gather all required evidence.** Use the bug or improvement requirements below. Redact before writing any body file.
7. **Create one issue per independent root cause or coherent proposal.** Use the matching repository template or the skill body template. Use label `bug` for defects and `enhancement` for improvements.
8. **Read it back.** Verify title, body, labels, public redaction, and URL:

   ```sh
   gh issue view ISSUE_NUMBER --repo ugur-murat-alt/figma-rust
   ```

9. **Link the result.** Include the issue URL in the final task report and in a related PR body/commit context when applicable.
10. **Handle failure honestly.** If GitHub auth, permissions, network, labels, or repository state blocks creation, preserve a sanitized ready-to-submit body and report the blocker. Never claim an issue exists without reading it back.

## Bug evidence requirements

Every bug issue must contain:

- concise title describing observed wrong behavior, not a proposed implementation;
- affected layer: plugin, core/IR, codegen, runtime, CLI/server, verification, capture/provenance, or OpenCode skill;
- repository commit SHA and `figma-rust version` output when available;
- OS/session type and relevant Rust, Node, Figma Plugin API, GPUI revision, scale/font/capture details;
- smallest sanitized input or fixture path and source node ID/property path when safe;
- exact command/API action and exit code;
- expected result and actual result;
- complete relevant diagnostic codes/messages, shortened only for unrelated repetition;
- deterministic reproduction steps;
- whether the failure is stable across two runs;
- applicable artifact hashes and verifier/provenance excerpt for visual defects;
- checks already run and their outcomes;
- explicit statement that secrets/private Figma content were removed;
- acceptance criteria for closing the defect.

Do not attach a full proprietary extraction when a minimal synthetic node or redacted subtree reproduces the bug.

## Improvement evidence requirements

Every improvement issue must contain:

- the user/operator problem and frequency;
- affected layer and current behavior;
- concrete proposed outcome without over-prescribing internals;
- alternatives/workarounds considered;
- compatibility, determinism, performance, security, and artifact-ownership impact;
- smallest fixture or benchmark that would prove value;
- observable acceptance criteria;
- test/verification plan;
- relationship to current plan/capability-matrix items;
- explicit statement that no private source material is included.

## Creation commands

Never submit the unchanged template. First create a private unique body file in
the operating system's temporary directory:

```sh
issue_tmp_root=$(mktemp -d "${TMPDIR:-/tmp}/figma-rust-issue.XXXXXX")
chmod 700 "$issue_tmp_root"
issue_body="$issue_tmp_root/body.md"
cp .opencode/skills/figma-rust/templates/bug-report.md "$issue_body"
chmod 600 "$issue_body"
```

Use the improvement template in the `cp` command for an actionable proposal.
Fill every required field in `$issue_body`, then review and redact it before
running either creation command below.

Bug:

```sh
gh issue create \
  --repo ugur-murat-alt/figma-rust \
  --title '[Bug] concise observed failure' \
  --label bug \
  --body-file "$issue_body"
```

Improvement:

```sh
gh issue create \
  --repo ugur-murat-alt/figma-rust \
  --title '[Enhancement] concise desired outcome' \
  --label enhancement \
  --body-file "$issue_body"
```

Delete the temporary directory after the issue has been created and read back.

## Duplicate and issue-count rule

- One root cause or one coherent proposal equals one issue.
- Multiple symptoms from the same root cause stay in one issue.
- Independent defects become separate issues only when each independently satisfies all evidence requirements.
- Never create an issue merely to record routine task progress, a successful run, or a question.
