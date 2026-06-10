-- trace_decompressor.lua — locate the graphics decompressor and its ROM source.
--
-- Stage 1: log the PCs that write the WRAM tile-staging area ($7F:C000-CFFF)
--          that the DMA loop later uploads to VRAM — those PCs are the
--          decompressor's store instructions.
-- Stage 2: while that staging area is being written (a cheap boolean gate),
--          record which ROM banks the CPU reads — that is where the compressed
--          graphics live.
--
-- IMPORTANT: do NOT call emu.getState() inside the ROM read callback — it fires
-- on every opcode fetch and building the state table millions of times crashes
-- the process. The read gate here is a plain boolean, set by the WRAM write.
--
-- Run headless and filter:
--   Mesen --testRunner rom.smc tools/mesen/trace_decompressor.lua 2>/dev/null | grep '^DTRACE'

local STAGE_LO, STAGE_HI = 0x7FC000, 0x7FCFFF
local FRAME_CAP = tonumber(FRAME_CAP) or 1500

local writers = {}            -- decompressor store PC -> count
local active = false          -- true the instant the staging area is written
local readmin, readmax = nil, nil
local banks = {}
local writes, reads = 0, 0

emu.addMemoryCallback(function(addr, value)
  writes = writes + 1
  active = true
  local st = emu.getState()   -- OK here: ~tens of thousands of calls, not millions
  local pc = ((st["cpu.k"] or 0) << 16) | (st["cpu.pc"] or 0)
  writers[pc] = (writers[pc] or 0) + 1
end, emu.callbackType.write, STAGE_LO, STAGE_HI, emu.cpuType.snes)

local function on_read(addr, value)
  if not active then return end
  local bank = (addr >> 16) & 0xFF
  if bank == 0x82 then return end             -- skip the decompressor's own code bank
  reads = reads + 1
  banks[bank] = (banks[bank] or 0) + 1
  if not readmin or addr < readmin then readmin = addr end
  if not readmax or addr > readmax then readmax = addr end
end
emu.addMemoryCallback(on_read, emu.callbackType.read, 0x008000, 0x3FFFFF, emu.cpuType.snes)
emu.addMemoryCallback(on_read, emu.callbackType.read, 0x808000, 0xBFFFFF, emu.cpuType.snes)

print("DTRACE|armed staging=$7F:C000-CFFF")

local n = 0
emu.addEventCallback(function()
  active = false               -- reset the gate each frame
  n = n + 1
  if n >= FRAME_CAP then
    print(string.format("DTRACE|writes=%d reads=%d romspan=%06X-%06X",
      writes, reads, readmin or 0, readmax or 0))
    local w = {}
    for pc, c in pairs(writers) do w[#w + 1] = { pc, c } end
    table.sort(w, function(a, b) return a[2] > b[2] end)
    for i = 1, math.min(#w, 12) do
      print(string.format("DTRACE|writer pc=%06X count=%d", w[i][1], w[i][2]))
    end
    local b = {}
    for bank, c in pairs(banks) do b[#b + 1] = { bank, c } end
    table.sort(b, function(x, y) return x[2] > y[2] end)
    for i = 1, math.min(#b, 12) do
      print(string.format("DTRACE|srcbank %02X reads=%d", b[i][1], b[i][2]))
    end
    emu.stop(0)
  end
end, emu.eventType.startFrame)
