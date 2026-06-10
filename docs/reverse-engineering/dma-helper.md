# Parameterized DMA setup — where the source pointers live

Static finding from `scan_dma_helper`, which clusters stores to the DMA
channel registers (`$43xx`) and classifies each register's value as a constant
(`immediate`) or as coming from memory/tables (`parameterized`). Raw report:
[reports/scan_dma_helper.json](reports/scan_dma_helper.json). Confidence
**likely** for the located variables (consistent, repeated, well-formed),
**speculative** for the interpretation until an emulator confirms it. This
extends [dma-transfers.md](dma-transfers.md), which only covered the four
immediate-fed transfers `scan_dma` can reconstruct.

## What the scanner found

**20 setup sites; 18 parameterized; 16 of those actually trigger a transfer.**
All of them live in **bank $00** (`$00:8264`–`$00:95xx`) — the early
init/loader code — and all use **absolute** register stores (none use a
`STA $43xx,X` channel loop). So the game does not have one generic indexed DMA
routine; it has many short, unrolled setup blocks that load the channel
registers from a small set of **direct-page and low-RAM variables**.

### Variables feeding the DMA *source* registers (A1TL/A1TH/A1B)

Tallied across the parameterized sites (frequency = how many register writes
read each variable):

| Variable | Width | Notes |
|---|---|---|
| `$E7..$E9` (DP) | 24-bit | most-used source pointer (A1TL/A1TH/A1B triple) |
| `$EA..$EC` (DP) | 24-bit | second 24-bit source pointer |
| `$16..$18` (DP) | 24-bit | another 24-bit source pointer |
| `$25..$26` (DP) | 16-bit | source word (bank set elsewhere) |
| `$1F4A/$1F4B`, `$1F4E`, `$1FE3..$1FE5`, `$1FA2..$1FA4`, `$1F0A/$1F0B`, `$1F24/$1F25` (low RAM) | 16/24-bit | per-upload source addresses staged in RAM |

The consecutive direct-page runs `$E7..$EC` and `$16..$18` are textbook
24-bit DMA source pointers (low, high, bank) held in zero page.

One operand, `$8412`, sits in ROM space (`$00:8412`) rather than RAM — a
single-hit lead that *might* be a pointer read straight from a ROM table.
**Speculative**; worth an `inspect_offset` look but not relied upon.

## Interpretation — likely

These variables are **not** the graphics storage; they are the *cursor* the
loader writes the current source address into right before each upload. That
the sources are computed into DP/RAM (not loaded as ROM immediates) is the same
signal seen in [dma-transfers.md](dma-transfers.md) and corroborates the
[[compression]] / decompress-then-DMA hypothesis in [../FORMAT.md](../FORMAT.md).

**The real graphics lead is now narrower:** find the code that *writes*
`$E7..$E9` (and `$EA..$EC`, `$16..$18`). Whatever computes those pointers is
the loader/decompressor, and the ROM address it derives them from is where the
graphics actually live.

## Next steps

1. Trace the writers of `$E7..$E9` / `$16..$18` (a "find stores to DP $E7"
   pass, or a breakpoint on those DP addresses once an emulator runs). That
   routine is the loader/decompressor.
2. `inspect_offset --snes 0x008412` to check whether `$00:8412` is a ROM
   pointer/table entry or a false hit.
3. Cross-check these against a live Mesen DMA log (`tools/mesen/dma_log.lua`)
   when a working emulator is available — the live `pc=` of each transfer
   should land inside the `$00:82xx`–`$00:95xx` sites listed here.
