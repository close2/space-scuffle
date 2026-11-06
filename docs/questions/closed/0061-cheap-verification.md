# Q0061: Who waits for the test gates?

- **Asked:** 2026-11-06 (given by the owner in chat, without a question being filed)
- **Blocking:** no
- **Default until answered:** not applicable; answered at once.

## Question

Implementation agents ran `cargo gate package` themselves and waited for it (often an hour behind the gate lock),
and the orchestrator woke every four minutes to watch. Both used the most capable model to wait.

## Answer

Owner (chat, 2026-11-06): "we are still waiting to much and are wasting intelligent models on waiting for tests.
improve the test procedures / gates (running them less often and only when necessary) and use cheap subagents for
waiting and verification"

## Resolution

2026-11-06. Charter §4.3: the implementation agent hands back after `cargo gate quick` and the tests its change
needs; a verifier agent on a cheap model runs `cargo gate package` on the final tree and reports the stamp or the
failing targets, which go back to the implementation agent. The orchestrator watches with event-only monitors
(gate finished, gate red, memory near the limit, a lock held unusually long) instead of a periodic heartbeat.
"Test a tree once" (Q0055) still applies: no tier reruns on a tree that already has a green stamp. Closed.
