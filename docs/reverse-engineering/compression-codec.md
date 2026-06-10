# Graphics compression codec — control-byte RLE (confirmed)

**Confidence: confirmed.** The per-command mechanics were read directly from the
routine's bytes (`cargo run --bin disasm`), and the whole codec — including the
two-pass/stride-2 bitplane-interleave layout — is now **round-trip-verified**
against real ROM data: a Mesen2 capture of a live decompress call (source
`$93:B9C9`, 6208-byte WRAM staging output) is reproduced byte-for-byte by the
committed decoder. The decoder lives at `src/codecs/gfx_rle.rs` (synthetic-fixture
unit tests); reproduce the live check with `tools/roundtrip.sh`. Raw listing:
[reports/disasm_decompressor.json](reports/disasm_decompressor.json).

This is the routine the [graphics pipeline](graphics-pipeline.md) identified as
the decompressor (`$82:8549` was the dominant-store window; the full routine is
`$82:84FD`–`$82:865F`). It reads compressed bytes from ROM banks `$92/$93/$95/$96`
and writes decoded bytes into the WRAM tile-staging area `$7F:C000…`.

## Register/direct-page layout

All operation runs with **8-bit accumulator, 16-bit index** (the entry does a
brief 16-bit `A` setup then `SEP #$20` at `$82:8505`).

| DP | Role |
|---|---|
| `$16`/`$17` + `$18` | **24-bit source pointer** into compressed ROM. After every byte read it is advanced by the macro `LDY $16 : INY : BNE + : INC $18 : LDY #$8000 : STY $16` — i.e. on 16-bit-offset wrap it carries into the bank byte and **resets the offset to `$8000`**. That is exactly LoROM contiguous-bank traversal, so the stream flows `$92:8000…$92:FFFF, $93:8000…`. |
| `$19`/`$1A` (+`$1B`) | **24-bit destination pointer** into WRAM. Advanced by **2** after every byte written (`LDY $19 : INY : INY : STY $19`) — the stride-2 interleave (see below). |
| `$1C`/`$1D` | Secondary destination base = initial dest **+ 1** (the odd-byte plane). Loaded by the end-of-pass command. |
| `$1F` | **Pass counter**, initialised to `2`. The end-of-pass command decrements it; the routine returns (`RTL` at `$82:8577`) when it reaches 0 ⇒ **two interleaved passes**. |
| `$00` | One-byte scratch (used by the pattern-fill command). |

### Stride-2 + two passes = bitplane interleave (likely)

Each decoded byte is stored two destination bytes apart, and the whole stream is
decoded **twice**: pass 1 fills the even bytes from base, then command `$40`
points the destination at base+1 and pass 2 fills the odd bytes. The result is
the standard SNES **2-bitplane interleave** (even byte = plane 0, odd = plane 1).
A 4bpp tile (4 planes) would need this run twice; the routine immediately
following at `$82:8662` (writes DP `$E4…`) is the suspected second-half / next
loader and is the next thing to disassemble.

## Command stream

The main loop (`$82:850B`) reads one command byte through `[$16]`, then
`AND #$E0` selects the operation from the **top 3 bits**; the **low 5 bits are a
length−1**, so every run is `N = (cmd & 0x1F) + 1`, range **1…32**.

| `cmd & 0xE0` | Handler | Operation | Stream bytes after cmd |
|---|---|---|---|
| `$00`, `$20` | `$8533` | **Literal copy** — copy the next `N` bytes verbatim to the output. | `N` |
| `$40` | `$8560` | **End of pass** — set dest = `$1C` (the odd plane), `DEC $1F`; loop for the next pass, or `RTL` when `$1F` hits 0. Length bits ignored. | 0 |
| `$60` | `$8578` | **2-byte pattern fill** — read bytes `B`,`S`; emit the pair `B,S` repeated `N` times (`2N` output bytes). | 2 |
| `$80` | `$85D6` | **Byte run (RLE)** — read byte `B`; emit `B` `N` times. | 1 |
| `$A0` | `$8604` | **Incrementing run** — read byte `B`; emit `B, B+1, …, B+N−1` (8-bit wrap). | 1 |
| `$C0` | `$85C0` | **Zero fill** — emit `0x00` `N` times. | 0 |
| `$E0` | `$8633` | **Decrementing run** — read byte `B`; emit `B, B−1, …, B−N+1` (8-bit wrap). | 1 |

This is a **custom control-byte RLE**, *not* an LZ scheme: there is no sliding
window, length/distance pair, or back-reference into already-output data. Every
command's output depends only on the command byte and at most two literal bytes.
That makes a decoder straightforward and an encoder fully deterministic — good
for a clean round-trip test.

## Reproduce

```sh
# full routine (entry runs 16-bit A, self-corrects at the SEP):
cargo run --bin disasm -- <rom> --snes 0x8284FD --end 0x82865F --m16 --x16
# just the dispatch + handlers (8-bit A, 16-bit X):
cargo run --bin disasm -- <rom> --snes 0x82850B --end 0x82865F --m8 --x16
```

## Next steps

1. ~~Write the decoder + round-trip test.~~ **Done** — `src/codecs/gfx_rle.rs`
   decodes, and `tools/roundtrip.sh` confirmed it reproduces a live Mesen2
   `$7F:C000` staging dump (source `$93:B9C9`) byte-for-byte. Compression is now
   *confirmed* in [FORMAT.md](../FORMAT.md).
2. Find the **graphics-id → source-pointer table**: locate the loader that sets
   DP `$16/$17/$18` (and the dest/`$1F`) before calling `$82:84FD`. That index is
   what a level/screen loader uses — the bridge from "a screen" to "these tiles".
3. Disassemble the follow-on routine at **`$82:8662`** (the `$E4` writer) — likely
   the second plane-pair / next stage of the same upload.
