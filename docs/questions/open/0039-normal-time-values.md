# Q0039: Should values copied in StarCraft II's Normal time be converted to our Faster-based ticks?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. The values below stay as they are, and the rest of the catalog is not
  audited.

## Question

**What we found.** The StarCraft II editor stores times and speeds in Normal-speed seconds. Our ticks are 1/25 of a
Faster second, and Faster is 1.4 times Normal. Four values were copied in Normal time without that conversion:

| Value | Ours | Reference in our ticks | Where | Effect on battle 33 |
|---|---|---|---|---|
| Marine (`pulse_bot`) speed | 0.09 per tick | **0.126** | catalog; doc 07's roster already says 0.1260 | −0.69 marine survivors (SE 0.44), small: no kiting |
| Zealot (`psi_grey`) shield regeneration | 0.08 per tick | **0.112** | catalog; doc 03 already says 2.8 per s | none (identical on all 68 seeds) |
| Shield regeneration delay | 250 ticks | **178.6** | hard-coded in `sim_core` (`world/upkeep.rs`) for every shielded unit | not run: not catalog data |
| Stimpack (`overclock`) duration | 250 ticks | **267.9** | catalog | not in battle 33 |

So these values hardly matter in battle 33, but they are factually wrong under the standing rule of Q0008. The
investigation checked only the marine and the zealot. The zealot's speed has the same editor value as the marine's
and was converted correctly, so other units may have either.

**Options.**

1. **No change (default).**
2. **Convert these four.** Three catalog edits and one `sim_core` change (the delay, best moved into data).
3. **Option 2 plus an audit** of every time and speed value in the catalog against the 5.0.14 game data, with a
   whole-matrix measurement before any value changes (recommended under Q0008).

**Side effects.** A faster marine changes fights where marines chase or run away; on its own it left battle 25b's
result unchanged. A shorter regeneration delay helps every shielded unit in long fights. In the whole-matrix
control (all 50 battles with every reference value of the marine and the zealot, including the speed and
regeneration rate), only battles 23a, 32, 33 and 25b moved, and the backswing alone explained 25b (Q0038). The
delay, the Stimpack duration and the audit were not measured.

Evidence: [`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md)
§1.3, §2.1, §2.4, §2.6, §5.3, §5.7 and §7.

## Answer

Owner (2026-11-04): Option 3 (convert these four Normal-time values to Faster-based ticks and audit every time and speed value in the catalog against the 5.0.14 game data, with a whole-matrix measurement).
