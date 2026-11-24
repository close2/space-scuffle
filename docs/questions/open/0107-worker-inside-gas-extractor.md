# Q0107: What happens to a worker while it is inside a gas extractor?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (1a), (2a), (3a): it survives its extractor's destruction and comes out;
  it sees nothing while inside; it comes out where it went in.

## Question

In StarCraft II a worker collecting gas disappears into the Refinery, Assimilator or Extractor for about
1.4 seconds, and while it is inside it cannot be attacked or hit by splash. Plan
`2026-11-24-hidden-harvesters` builds that. The game data say the worker is hidden but not the details
below, and we found no page that does. Each choice is small and easy to change.

1. **The extractor is destroyed while a worker is inside.**
   - (a) The worker survives and appears where it went in (default).
   - (b) The worker dies with the extractor.
2. **Sight while inside.**
   - (a) The worker sees nothing; the extractor's own sight (9) covers almost the same area (default).
   - (b) The worker keeps its own sight (8) from where it stands.
3. **Where it comes out.**
   - (a) Where it went in, which is normally the side facing its base (default).
   - (b) On the side facing the base it returns to, wherever it went in.

If you can check in the game or editor: with an SCV inside a Refinery, kill the Refinery, and watch
whether the SCV survives and where it appears. Also: send a single SCV to a Refinery in fog, and see
whether the fog stays lifted beyond the Refinery's own sight while the SCV is inside.

## Answer

