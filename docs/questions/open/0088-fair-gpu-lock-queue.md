# Q0088: May the shared `with-gpu-lock` script be replaced by the queued version (fair GPU lock, B-136)?

- **Asked:** 2026-11-18
- **Blocking:** no
- **Default until answered:** the script stays as it is. Agents keep at least 60 s between consecutive
  `with-gpu-lock` holds in a series (`docs/guides/gpu-and-disk.md`), and B-136 stays open. The test harness already
  contains the matching queue code, which stays switched off until the installed script names the queue.

## Question

`flock` is not fair. A GPU test child waited its whole 290 s timeout for `/run/lock/gpu.lock` behind a series of
short `with-gpu-lock` captures, and failed a package gate (B-136; plan `docs/plans/2026-11-17-gpu-test-readback.md`
Amendments 4 and 5). Every taker of the lock on this host should be served in arrival order.

The proposed fix keeps `flock` for exclusion and adds a ticket queue in `/run/lock/gpu.queue`. Only the earliest
live ticket may try the lock; dead or expired tickets are ignored. A taker that does not know the queue can still
cut in, as today, so exclusion is never weakened. The complete script, its diff and its failure modes are in
Amendment 5 §C. The proposed file is `~/scratch/gpu-ledger/proposal/with-gpu-lock` (on this server). It was
tested on private paths: FIFO order held, and a waiter got the lock second during a back-to-back series where
today's script starved it behind all 16 holds.

`/usr/local/bin/with-gpu-lock` is owned by root and is shared with the other autonomous agent user on this server,
so agents neither can nor may install it. Installing it also changes that user's waiting, in their favour as much
as ours.

## Options

- **(a)** The owner installs the proposed script (as root), after telling the other user's owner. The harness
  starts queueing by itself; the 60 s rule is then dropped and B-136 closed.
- **(b)** Keep today's script and the 60 s rule (the default).
- **(c)** Something else, such as a change to the proposal.

## Answer

**Option (a) — Approved.**

The owner will deploy `~/scratch/gpu-ledger/proposal/with-gpu-lock` to `/usr/local/bin/with-gpu-lock` (along with a `tmpfiles.d` entry `d /run/lock/gpu.queue 1777 root root -` so `/run/lock/gpu.queue` is pre-created owned by `root` across reboots) via the `infrastructure` project (`quorra`'s owner is the same owner and `quorra` already calls `/usr/local/bin/with-gpu-lock` directly). It may take a short while until the `infrastructure` playbook run applies it: once `grep -q gpu.queue /usr/local/bin/with-gpu-lock` succeeds on this VM, drop the temporary 60 s spacing rule in `docs/guides/gpu-and-disk.md` and close `B-136` and `Q0088`.
