# Q0055: How do we spend less time on tests?

- **Asked:** 2026-11-05 (given by the owner in chat, without a question being filed)
- **Blocking:** no
- **Default until answered:** not applicable; answered at once.

## Question

Full gates on `main` ran after nearly every merge, often on trees an agent's gate had just tested, and several
agents built and tested in parallel, which made each of them slow (and, on 2026-11-05, froze the server).

## Answer

Owner (chat, 2026-11-05): "to reduce testing time: push less often, don't run tests when nothing changed since last
test run (tests have been run and we merge only) and run less agents in parallel, so that they are faster (instead
of crawling in parallel)"

## Resolution

2026-11-05. Charter §4.3 gains "Test a tree once": a gate tier passes without running when the same tree already
has a green stamp of that tier or a stricter one, and an agent's green `package` stamp counts for `merge` and `full`
when the merged tree is the tested tree. Pushes are batched; at most one or two building agents run at once; one
`cargo gate` runs at a time through the gate lock. The tool change is a short-note package (tiered-gates plan,
Amendment 3). Closed.
