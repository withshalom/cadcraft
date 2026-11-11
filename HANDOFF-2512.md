# Handoff: storytold/cadcraft#2512 (DWG in the web build)

Written 2026-11-11. This file lives only on `handoff/acadrust-web-2512`, never on the PR branch, so it
stays out of the PR diff. Delete the branch when the PR merges.

## Where things stand

- PR: https://github.com/storytold/cadcraft/pull/2512, head `withshalom:acadrust-web`, closes #898 (FF12).
- Branch tip when this was written: `12067232`. The merge of upstream `main` is `9c5196cc`.
- **The branch does not compile.** `apps/cadcraft-web/src/host.rs`: `post_open_failed` calls
  `Bridge::send`, which no longer exists since upstream moved the host onto `craft-web-host`
  (pinned `storytold/craft-libs` rev `fd5e2fe`, PR #420 there). Nothing has been built, linted or
  tested since the merge: the session had no access to the git dependencies (the egui fork) or a
  wasm32 target.
- No Claude/agent attribution in commits or the PR description: AGENTS.md rule 15.

## What the PR does

1. `crates/dwg`: acadrust is a normal dependency on every target except FreeBSD aarch64; `build.rs`
   sets `no_dwg_bridge` only there; wasm stand-ins removed.
2. `crates/io/Cargo.toml`: the acadrust dev-dependency uses the same target as `cadcraft-dwg`, as the
   new `xtask/tests/dwg_dependency_targets.rs` requires.
3. `cadcraft:open-failed { name, error }` to the embedding page when a drawing it sent doesn't open
   (#898's second request). **Broken by the merge: see below.**
4. `CadApp::open_bytes` returns its result; a `.lsp` file that fails to load returns the error
   (test: `a_lisp_file_the_reader_rejects_returns_the_error`). This answers CodeRabbit's finding.
5. `contributors/people.toml`: the `withshalom` entry (sits below Helen Kwok's upstream entry).

Parity docs were deliberately dropped from the PR (CLAUDE.md: PRs don't edit them), so the
description names the rows instead.

## To do before marking ready

1. Port `post_open_failed` (and the `from_host` plumbing in `apps/cadcraft-web/src/upload.rs`) to
   `craft-web-host`'s `Host` API. Known so far: `post_ready(version, false)`, `post_failed(error, None)`,
   `post_dirty(bool)`, `post_save(name, bytes, None, false)`, `listen(|Message::Open { name, data }| ..)`.
   Not known: whether it can send a per-file failure message. If not, the wire message belongs in
   `craft-web-host` (a PR to `storytold/craft-libs`) and this PR waits on it, or drops item 3.
   Read the crate at the pinned rev before writing anything.
2. `cargo xtask ci` on the current tip (fmt, clippy -D warnings, tests, assets, layers, wasm).
3. Measure the wasm package size before and after (`trunk build --release` in `apps/cadcraft-web`).
   JanuschC measured about +7 MB (14 to 21 MB, no `wasm-opt`) on a different viewer.
4. Open two or three real DWGs in the web build (cadcraft.dev / dwg.cadcraft.dev) and note the
   results. The only in-browser evidence so far is a synthetic LINE round trip (2000 to 2018) in
   `wasm-bindgen-test` under Node, run by hand on the first commit.
5. Paste the description below into the PR, adjusting item 3 to what actually shipped.

## Known limits (state them in the PR, do not hide them)

- wasm32 panics abort the page; `catch_unwind` does nothing there. File-declared sizes can drive
  allocations the size caps don't bound. CodeRabbit rated this moderate (inferred, no exploit shown).
- Follow-up, not this PR: decode DWGs in a Web Worker, the web counterpart of
  `crates/dwg/src/worker.rs` (the native child process).
- Opens are capped at `MAX_OPEN_BYTES` (256 MB); #2427 and #1686 own the limits.

## People to credit

- @alangaria proposed the same removal in #1378 (no longer applies to `main`).
- @tnorverto reported #898.
- @JanuschC proposed the same change in issue #2929 (an issue, not a PR; his branch
  `JanuschC/cadcraft:feat/dwg-in-web-build`, commit `bd39381d`). His code was already identical to
  ours; two comment wordings came from it (commit `12067232`). His measurements are his.
- The ticket bot marked #2929 a duplicate of #898 and pointed at #2512.

## Practical notes

- GitHub says the fork moved: `withshalom/cadcraft` is now `withshalom/cadcraft-artcraft`. Pushes
  through the old URL still worked.
- The previous session could not edit the PR: `storytold/cadcraft` and `withshalom/cadcraft` can't both
  be attached (same repo name). To edit the PR body, start a session with `storytold/cadcraft` as the
  source, or paste the description by hand.
- `plan/` is a gitignored notebook; there is none in this checkout.

## PR description (paste, then adjust item 3)

```
## DWG in the web build: build the acadrust bridge for wasm32 (FF12, closes #898)

The web build refuses every DWG ("DWG files can't be opened in the web build yet; save as DXF"),
although the desktop app and the CLI read and write them. #898 describes the cost for a studio that
embeds the web build: each DWG is converted server-side first, the DXF is 4-8x larger, and plans of
about 45 MB reach the browser's limit.

`acadrust` already builds for `wasm32-unknown-unknown`: it uses `web-time` for its clocks, runs its
parallel stages sequentially on wasm, and keeps file memory-mapping to native targets. The bridge
never used memory-mapping (it passes bytes through a `Cursor`), so the only thing keeping DWG out of
the browser was the bridge's own `cfg` gate.

### What this changes
- `crates/dwg/Cargo.toml`, `build.rs`: `acadrust` is a normal dependency except on FreeBSD aarch64
  (same version, still unmodified); `no_dwg_bridge` is set only there.
- `crates/dwg/src/lib.rs`: removes the wasm32 gates and the wasm stand-ins that returned the
  "save as DXF" error. The bridge, its limits and its guards are unchanged.
- `crates/io/Cargo.toml`: the acadrust dev-dependency uses the same target as `cadcraft-dwg`
  (`dwg_test_dependency_matches_the_bridge_targets`).
- #898's second request, tell the embedding page when a drawing it sent doesn't open: with
  `?host=parent`, a refused `cadcraft:open` (no bytes, too large, unreadable) also posts
  `cadcraft:open-failed { name, error }` to the parent, with the status-bar text. Files the user
  picks or drops are unchanged. [ADJUST: true only once ported to craft-web-host]
- `CadApp::open_bytes` returns the outcome; a `.lsp` file that fails to load is returned as an error
  too, with a test.
- `contributors/people.toml`: my entry.

### Docs rows affected (not edited here, per CLAUDE.md)
`gaps.md` FF12 and P1; `file-format-parity.md` web build row; `architecture.md` `dwg` and
`cadcraft-web` rows; `target-app-parity.md` Platforms row.

### Measured
[FILL IN after the to-do list: wasm size before/after, real DWGs opened in the browser, `cargo xtask ci`.]
Earlier, by hand on the first commit only: a LINE written as DWG at 2000, 2004, 2007, 2010, 2013 and
2018 and read back inside WebAssembly (`wasm-bindgen-test` 0.2.129, Node 22); a damaged DWG returns an
error. R13 and R14 are untested in the browser.

### Limits in the browser
- A panic aborts the page on wasm32, so the bridge's `catch_unwind` guards don't apply there. The size
  limits still apply before the decoder runs.
- Some allocations are driven by sizes declared inside the file, and the size caps don't bound the
  decoder's internal allocations. Decoding in a Web Worker (the web counterpart of `worker.rs`) would
  contain both; I'd do that as a follow-up.
- Opens are capped at `MAX_OPEN_BYTES` (256 MB). #2427 and #1686 already work on the limits.

### Credit
@alangaria proposed the same removal in #1378, which no longer applies to `main`. @JanuschC proposed
the same change in #2929 (wasm package about +7 MB, 14 to 21 MB without `wasm-opt`; real DWGs opened
in Chromium with the same element counts as native); two comment wordings here come from his branch.
Thanks to @tnorverto for the report in #898.

Closes #898.
```
