# Q0025: Commit generated media as plain git files, or move media to Git LFS?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** commit the required set's clips and atlases as plain git files (about 160 MB more), and
  ask again before the expansion batches.

## Question

Each unit atlas is a 512 px PNG strip of 9 to 18 MB, and each Veo clip 1 to 3 MB. Today they are plain git files:
there is no `.gitattributes`, the git pack is 153 MiB and `assets/textures` is 221 MB on disk. The plan
`docs/plans/2026-11-04-phase-8-5-veo-media.md` adds roughly 160 MB for the required set and roughly 600 MB more if the
whole Automaton faction gets media. Rejected clips stay on `~/scratch` and are not committed.

- **(a) Plain git files (default for the required set).** Simple; every clone grows; no file comes near GitHub's
  100 MB limit.
- **(b) Git LFS for `assets/textures/*.png` and `assets/videos/*.mp4`.** Clones stay small, but the remote's LFS
  storage and bandwidth quotas apply and CI must fetch LFS objects. Old blobs stay in history either way.
- **(c) Smaller atlases** (for example 256 px frames for structures). Saves space, changes how they look up close.

## Answer

Owner (2026-11-04): Agreed with agent's recommendation — Option (a): commit the required set's clips and atlases as plain git files, and ask again before the expansion batches.
