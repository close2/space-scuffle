# Questions for the Owner

This directory is how agents and the owner talk to each other when they are not in the same session. Every
decision that belongs to the owner lives here as one file, so nothing depends on a chat log.

## For the owner

**To see what is waiting:** open [`open/`](open/). Each file is one question.

**To answer:** write your answer under `## Answer` in that file and commit it to `main`. The GitHub web editor is
enough. A sentence is fine, and so is "default is fine". You can answer in any order, or not at all.

**To ask an agent for something, or change your mind:** add a file to `open/` yourself, or edit a closed question
and move it back. Any format works.

If you answer in chat instead, the agent copies your answer into the file.

## For agents

1. **Check at the start of every session, and before every push.** Run `git pull`, then read `open/`.
2. **Act on answered questions.** A question is answered when `## Answer` has text. Carry it out, add a
   `## Resolution` section saying what changed and in which commit, and move the file to `closed/` with `git mv`.
3. **Ask here, not only in chat.** Anything the owner must decide gets a file. Mention it in your final message too.
4. **Do not stall.** Every question states the default you are following. Keep working on that default, and choose
   work that is easy to change if the answer differs.
5. **Stop only for `Blocking: yes`.** Use it when no safe default exists, for example spending money or an
   irreversible action. Do not do that one thing, say so in chat, and carry on with everything else.
6. **Never write secrets here.** This directory is pushed to a remote.

An agent only sees an answer when it next pulls, so answers are picked up during a session's next push or at the
next session, not instantly. If something is urgent, say it in chat.

## Format

Files are named `NNNN-short-slug.md`. Numbers only go up, and are shared by `open/` and `closed/`.

```markdown
# Q0001: One-line question

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** what agents do meanwhile

## Question

What needs deciding, with the options and a recommendation. It must make sense on its own, to a reader who has
not seen the session.

## Answer

```

The directory is the status: a file in `open/` is waiting, and a file in `closed/` is done. There is no index to
keep up to date.
