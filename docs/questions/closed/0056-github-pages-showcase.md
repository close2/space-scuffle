# Q0056: Enable GitHub Pages for `close2/space-scuffle` to host the showcase video?

- **Asked:** 2026-11-05 (in chat, by the orchestrator)
- **Blocking:** no
- **Default until answered:** enable Pages (source: GitHub Actions).

## Question

Plan [`2026-11-05-ci-showcase-video`](../../plans/2026-11-05-ci-showcase-video.md) (D-10) publishes the CI-rendered
showcase through GitHub Pages at `close2.github.io/space-scuffle`: nothing is added to git history, the README
embeds an animated WebP teaser that links to a page with the full MP4, and an agent enables Pages with `gh api`.
Alternatives: an orphan `showcase` branch with raw URLs (clones fetch about 25 to 30 MB more per update), or Pages
with a nightly render instead of one after each green CI run on `main`.

## Answer

Owner (chat, 2026-11-05): "Enable GitHub Pages (Recommended)".

## Resolution

2026-11-05. Pages is enabled by the plan's WP5; renders run after each green CI run on `main`, skipping runs where
nothing shown changed. Closed.
