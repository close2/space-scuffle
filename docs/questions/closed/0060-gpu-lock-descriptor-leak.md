# Q0060: Should `with-gpu-lock` stop passing the GPU lock to the commands it runs?

- **Asked:** 2026-11-06
- **Blocking:** no
- **Default until answered:** no change to the shared script. Our own tools make sure nothing they start under
  `with-gpu-lock` outlives the command (each showcase take's process group is killed and reaped when it ends), and
  the GPU test harness takes the lock in process and does not pass it to test children.

## Question

`/usr/local/bin/with-gpu-lock` (root-owned, shared with the other agent user) ends in
`exec flock --timeout "${GPU_LOCK_TIMEOUT:-600}" "$LOCK_FILE" "$@"`. Without `-o`, the lock's file descriptor is
inherited by every process the command starts. If one of them outlives the command (an orphaned helper, a stuck
Xvfb, ffmpeg or test child), `/run/lock/gpu.lock` stays held until that process exits, and the other user's GPU
work waits. The same mechanism held our own gate lock for about 10 minutes on 2026-11-05 (orphaned `sleep`
processes from a test). No case on the GPU lock has been observed, and nothing logs its holders, so we cannot tell
whether it happened during the 2026-11-05 freeze (13:47–15:04 UTC), when our hung test children may also have held
it.

**Options.**
1. Change the script to `exec flock -o --timeout "${GPU_LOCK_TIMEOUT:-600}" "$LOCK_FILE" "$@"` (one word; `flock`
   keeps holding the lock, the command and its children do not get the descriptor). Needs root; affects both users.
2. Option 1, plus log each acquisition and release (time, user, PID, command) to a small file, so lock waits can be
   answered from data.
3. Keep the script as it is (default); our side avoids leaks as described above.

**Recommendation:** 2.

## Answer

Owner (2026-11-06): Option 2 — updated `/usr/local/bin/with-gpu-lock` in `ansible/agents.yml` and applied to the VM so the lock file descriptor (FD 200) is closed in the spawned command (`200>&-`) and each acquisition, timeout, and release (with UTC timestamp, user, PID, exit code, duration in ms, and command) is logged to `/run/lock/gpu-lock.log`.

## Resolution

2026-11-06. Option 2, applied by the owner on the VM through `ansible/agents.yml` (outside this repository).
`/usr/local/bin/with-gpu-lock` now takes the lock on descriptor 200 in its own shell, runs the command with that
descriptor closed (`"$@" 200>&-`) instead of `exec flock … "$@"`, and releases the lock when the command exits. A
process the command leaves behind therefore cannot hold `/run/lock/gpu.lock`. Each acquisition, timeout and release
is logged to `/run/lock/gpu-lock.log` (UTC time, user, PID, command; on release also the exit code and the duration
in milliseconds), so lock waits can be answered from data.

Our side, kept as well: each showcase take's child leads its own process group, which is killed whole and reaped
when the take ends (`fc49f83`, confirmed on `radv` by render r7 in `7126e40`, merged in `4633471`); the gate lock
tests do the same for the lock holder (`d8c0e06`). Charter `AGENTS.md` §7 states the new behaviour of
`with-gpu-lock`.

Closed.
