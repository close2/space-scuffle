# Q0054: Which changes need a full plan, and which only a short note?

- **Asked:** 2026-11-05 (given by the owner in chat, without a question being filed)
- **Blocking:** no
- **Default until answered:** not applicable; answered at once.

## Question

Charter §6 required a planning sub-agent and a full blueprint in `docs/plans/` before every change, however small.

## Answer

Owner (chat, 2026-11-05): "slight adjustment to the plan rule: Require a full plan for: new subsystems or mechanics,
anything that moves battle outcomes or golden replays, anything that changes the checksum format, and multi-package
work. Allow a short amendment or a decision note in the commit for: small, well-specified changes. Examples: config
and data tweaks you dictated, test-only fixes, docs, and single-cause bug fixes with an obvious test. Let the
orchestrator write that short note itself, instead of spawning a separate planning agent each time."

## Resolution

2026-11-05. Charter §6 now has the two levels. The orchestrator still never edits code itself; implementation stays
with implementation sub-agents. Closed.
