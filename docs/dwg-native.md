# dwg-native: DWG as CADCraft's own format

> **Status:** charter, no work yet · **Branch:** `dwg-native`, a sister of `cad-native`; both pull
> from `dwg-fidelity`.

The question this branch answers: can DWG be CADCraft's native save format, the one it saves by
default and treats as its own, with nothing important lost? `cad-native` answers the same
question with `.cad`. They are alternatives; neither blocks the other.

`dwg-fidelity` stays conservative (CADCraft's model and behaviour unchanged, only better DWG
fidelity). Changes that alter what CADCraft does, not just how well it holds DWG, belong here.

## Open questions to settle first

1. **What DWG can't hold.** Constraints, parametric data, layer states, plot style tables and
   other CADCraft-only data have no DWG home. DXF files already carry them as extra data; the
   same must work inside DWG, and other programs may drop it. Measure what is lost on a
   save-and-reopen through AutoCAD-style readers.
2. **Safe saving.** Every save goes through the DWG writer: read back and verify before
   replacing the old file, and keep the previous file until the check passes.
3. **Direct translation.** Replace the DXF text step between DWG and the model, with the old
   path as a fallback, equal on every corpus file.
4. **Default save.** DWG 2018 as the default for new drawings, with the choice visible to the
   user.
5. **Locks.** AutoCAD's `.dwl` beside DWG files: warn when a foreign lock is present (agreed);
   decide whether CADCraft writes its own.

## Rules

- Pull from `dwg-fidelity`; don't merge `cad-native` here.
- Same standards as the rest of the fork: no panics in non-test code, clean-room, `cargo xtask ci`.
