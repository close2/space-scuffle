# Q0065: How fast does a `ResetIfMoving` turret return when its turret sets no `YawIdleRate`?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** no change. The Siege Tank (tank mode), Hellion and Phoenix analogues' turrets keep their
  last angle when they have no target (`Hold`), as every turret did before WP10. Their return to the front stays
  recorded under bug B-30 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP10 (bug
B-30) models what a turret does when it has no target, from `CTurret` `Idle` and `YawIdleRate` in the 5.0.14 data
(evidence [`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §11.4). The SC2Mapster
editor reference ([Data/Turrets](https://sc2mapster.wiki.gg/wiki/Data/Turrets), read 2026-11-09) defines `Idle` ("what
the turret does when it does not have a target": `Hold`, `Reset`, `ResetIfMoving`, `FollowMovement`, `Spin`) and
`YawIdleRate` ("speed of the turret's movements when it does not have a target"). Three turrets of matrix
identities use `ResetIfMoving` but set no `YawIdleRate`:

- `SiegeTank` (`liberty` TurretData:77-79), the tank-mode cannon of the Siege Tank analogue;
- `Hellion` (`liberty` TurretData:95-98);
- `Phoenix` (`swarm` TurretData:31-39).

No layer's `CTurret` default sets `YawIdleRate` (`core` TurretData:3-10), and the reference states no default. So
the engine's value is unknown: 0 would mean the turret never returns; the turret's `YawRate` is another plausible
reading.

**Why it matters.** It decides where these turrets point when a new target appears after the unit has moved, and so
how long they turn before their first shot (the acquisition arc of B-29 is 0 for the Hellion).

**Options.**

1. **No change (default).** The three turrets hold their angle.
2. **`YawRate`.** They return to the front at their targeting rate (55.99, 30.20 and 40.32 degrees per tick).
3. **Measured.** An editor test: a Siege Tank that has fired to its side is ordered to move; log the turret's yaw
   each game loop.

**Recommendation.** Option 3, then option 2 or the measured rate as data; the rule is already in the code.

## Answer

**Option 2 (`YawRate` — return to front at their targeting rate; no editor test needed).**

**Engine insight:** In StarCraft II's `CTurret` implementation, `YawIdleRate` is an optional override used primarily by decorative/continuous-spin turrets (`Idle = Spin`, such as the Missile Turret or Sensor/Radar Tower). When a combat turret has `Idle = ResetIfMoving` or `Reset` (such as the Siege Tank in tank mode, Hellion, or Phoenix) and leaves `YawIdleRate` unset (`0` in XML), the engine falls back to the turret's normal `YawRate` (`55.99°`, `30.20°`, and `40.32°` per tick) to rotate the turret back to its forward `YawStart` (`0°`) alignment. Fall back to `yaw_rate` when `YawIdleRate` is unset (`0`).
