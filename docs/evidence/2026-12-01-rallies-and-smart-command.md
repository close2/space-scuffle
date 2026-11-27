# Evidence: the remaining rally rows and the per-unit smart command (B-163, B-167, B-214, B-215, B-217, B-222)

Plan: [`docs/plans/2026-12-01-rallies-and-smart-command.md`](../plans/2026-12-01-rallies-and-smart-command.md).
Measured on the package branch `rallies`, cut from `main` `b2ba4621`, from 2026-11-27, on the shared VM. Scratch
material lives under `~/scratch/rallies/smart/` (`scripts/`, `bin-<wp>/`, `tree-<wp>/`, `world-<wp>/`,
`client-<label>/`, `red/`, `valgrind-<label>/`); the scripts are adapted from the follow-order plan's
(`~/scratch/follow-order/scripts/`).

## 1. WP0: records, probe and baseline

### 1.1 Records

- **Numbers.** `main` ends its register at B-245. The open branches claim B-246 and B-250 (`diagonal-cliffs`), and
  the minimap plan proposes B-248 and B-249 without filing them. B-247 occurs on `main` and on every open branch only
  in this plan's own text, so the plan's provisional B-247 stands and nothing is renumbered. Q0140 and Q0141 were
  already filed on `main` (`0140-unit-exit-placement.md`, `0141-rally-waypoint-cap.md`), so they keep their
  numbers.
- `DESIGN.md` §4: B-163's status is **fixed** (follow order WP3 `0a797bab` and WP4 `27604c11`), with "open;" and
  "stays open under …" struck; B-214 is removed (sanctioned below); B-247 (a transport's right click does not pick a
  unit up, deferred to WP-D1) is registered; B-167, B-215, B-217 and B-222 point to the plan.
- Q0052 gains the addendum **S-rally-keys** (D-5), from Q0115 (a).

### 1.2 Worlds (`world-wp0/`)

`scripts/wp.sh wp0 b2ba4621` exports the commit (WP0 changes only documents, so its code is `b2ba4621`'s) to
`tree-wp0/`, builds `matchup_trace` and `route_trace` in release in a separate target directory
(`/home/spacescuffle/cargo-target/rallies-measure`) and runs `scripts/measure.sh` (the follow-order plan's: the matrix
with every battle on its seeds of record and 16 seeds, battle 31 on 100 seeds, battle 33 on 64, the showcase and
showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks): 483 files, the reference for every
later comparison by `scripts/compare.sh` (every output file byte for byte; logs after removing the output path). The
matrix log reads "1220 runs: expectation failures 2 on the seeds of record, 26 over all seeds", as on `main`.

### 1.3 P-0: no skirmish right click is resolved differently per member

`scripts/probe.sh b2ba4621` exports the commit twice. It runs the client skirmish match
(`skirmish_match_tests::a_match_is_won_through_real_input_alone`, release) on the unpatched copy (`client-wp0/`), and
on a copy patched by `scripts/patch_probe.py` (`client-probe/`, never committed). The patch adds one line to
`issue_pointer_order`, after today's `context_order` and rally choice, that compares, member by member, the order
today's single-order rule gives (the members `any_recipient` accepts for it, when the unit submission happens at all)
with the order D-1's per-member table would give (the first row each member takes, by the same member rule), and
prints whether they differ. Today's behaviour is unchanged.

| Lines | Count |
|---|---:|
| `P0PROBE click` (right clicks) | 8 |
| `differs=true` | **0** |

The eight clicks are the pilot's six workers sent to minerals at tick 0, one at tick 594, and the army's six units
attacking an enemy at tick 5,542: selections of workers alone or of the army alone, as plan §6 expected. P-0 is
zero, so stop rule 1 does not apply and §7's skirmish row stands.

### 1.4 The skirmish match (`client-wp0/`, `client-probe/`)

On WP0's tree the match is won at tick 6,342 (1,645 frames), the pilot's stages at ticks 0, 0, 0, 22, 550, 586, 642,
4,502, 4,526, 4,822, 4,834, 5,086, 5,098, 5,350, 5,362 and 6,334, the figures of the follow-order evidence. The
probe's tree gives the identical stage log.
