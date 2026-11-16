# Q0084: May agents shorten AGENTS.md (the charter) without changing any rule?

- **Asked:** 2026-11-16
- **Blocking:** no
- **Default until answered:** AGENTS.md stays as it is. Agents shrink only the other files they read: they move
  plan amendments and review rounds into their own files, move the bug register out of `DESIGN.md`, write briefs
  that name the sections to read, and tidy stale memory notes.

## Question

AGENTS.md is loaded into every agent's context, and subagents also read long plans and packets. The setup
evaluation (2026-11-16, item 6) measured:

- the effect plan is 265 KB (about 66,000 tokens), and 63 % of it is 29 amendments;
- the review packet is 164 KB;
- the bug register in `DESIGN.md` is 14.6 KB, and `docs/plans/README.md` is 48 KB;
- a subagent turn holds a median of about 256,000 tokens.

Most of the saving comes from those other files, which the agents may reorganise themselves. The charter could also
be tightened, for example §7's long GPU paragraphs could move to a linked how-to, but §6 says its rewrites belong to
you.

Options:

1. **Leave the charter (default).** Compact only the other files.
2. **Allow an editorial compaction.** The same rules and checks, said in fewer words, with long how-to text moved
   to linked files. The diff would be shown to you in a question before it is merged.

## Answer

