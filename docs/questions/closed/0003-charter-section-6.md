# Q0003: Should charter §6 stop naming model tiers?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** §6 stays as committed. Agents follow its two stages and use the session's model.

## Question

§6 requires `pro` and `flash` sub-agents. Those names do not exist here, so the rule cannot be followed literally.
The owner discarded the uncommitted patch that would have named a Gemini model, so that part is gone.

The recommendation (remediation decision D8) is to make §6 tool-neutral: keep plan first and implement second, and
drop the model names. Agents will not rewrite §6 without your say.

## Answer

Owner (2026-11-03): I agree with recommendation. Naming models did not work as I wished.

## Resolution

2026-11-03, commit `ab53c32`. `AGENTS.md` §6 no longer names `pro`, `inherit` or `flash`. It keeps both stages: a
planning agent writes the blueprint in `docs/plans/` first, and implementation agents follow the approved blueprint
strictly. A new item 3 says the charter names no model or tool for either stage. The remediation plan's decision D8
records the approval.
