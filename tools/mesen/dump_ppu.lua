-- dump_ppu.lua — reach gameplay, then dump the live PPU state so the editor's
-- STATIC reconstruction (src/level/loader.rs) can be validated against the real
-- machine: CGRAM (the palette), the BG1 character base + tilemap config, and a
-- slice of VRAM.
--
-- Input driving is copied from trace_play.lua (pulse START/A/RIGHT until the
-- in-level object iterator $80:E9A8 runs). We then wait SETTLE frames for any
-- fade-in / palette animation to finish before dumping, and emit hex lines:
--
--   PDUMP|level=<n>
--   PDUMP|ppu <key>=<val> ...        (BG1 char base, tilemap addr, bg mode)
--   PDUMP|cgram <512 hex bytes>      (256 BGR555 colors, low byte first)
--   PDUMP|vram <off> <hex...>        (VRAM bytes from $0000 in 64-byte rows)
--
--   MESEN_BIN=... ./run-headless.sh <rom> dump_ppu.lua 240 | grep '^PDUMP'

local OBJ_ITER = 0x80E9A8
local FRAMES   = tonumber(FRAMES) or 6000
local SETTLE   = tonumber(SETTLE) or 180   -- frames to wait after reaching level

local function r8(a)  return emu.read(a & 0xFFFF, emu.memType.snesMemory) end
local function r16(a) return r8(a) | (r8(a + 1) << 8) end

local frame = 0
local in_level_frame = -1
local dumped = false

emu.addMemoryCallback(function()
  if in_level_frame < 0 then
    in_level_frame = frame
    print(string.format("PDUMP|IN-LEVEL at frame %d level=%d", frame, r16(0x1EEA)))
  end
end, emu.callbackType.exec, OBJ_ITER, OBJ_ITER, emu.cpuType.snes)

local function pulsing(period) return (frame % period) < (period // 2) end

local function hex_run(memType, off, len)
  local parts = {}
  for i = 0, len - 1 do
    parts[#parts + 1] = string.format("%02X", emu.read(off + i, memType))
  end
  return table.concat(parts)
end

local function dump()
  print(string.format("PDUMP|level=%d", r16(0x1EEA)))

  -- PPU layer/state. getState() is flat-keyed in Mesen2; print every ppu key
  -- that looks like a BG1 tile/char address or the background mode so we can
  -- read the real character base regardless of exact key naming.
  local st = emu.getState()
  local keys = {}
  for k, _ in pairs(st) do
    local lk = string.lower(k)
    if lk:find("bgmode") or lk:find("ppu.bglayer") or
       (lk:find("ppu") and (lk:find("tile") or lk:find("char") or lk:find("chr"))) then
      keys[#keys + 1] = k
    end
  end
  table.sort(keys)
  for _, k in ipairs(keys) do
    print(string.format("PDUMP|ppu %s=%s", k, tostring(st[k])))
  end

  -- CGRAM: full 512 bytes (256 colors).
  print(string.format("PDUMP|cgram %s", hex_run(emu.memType.snesCgRam, 0, 512)))

  -- VRAM: dump $0000..$8000 (the BG tile area) in 64-byte rows.
  for off = 0, 0x7FC0, 0x40 do
    print(string.format("PDUMP|vram %04X %s", off, hex_run(emu.memType.snesVideoRam, off, 0x40)))
  end
end

emu.addEventCallback(function()
  frame = frame + 1
  local inp = {}
  if frame > 120 then
    if pulsing(24) then inp.start = true end
    if frame > 600 then
      if pulsing(32) then inp.a = true end
      if pulsing(40) then inp.right = true end
    end
  end
  emu.setInput(0, inp)

  if not dumped and in_level_frame >= 0 and frame >= in_level_frame + SETTLE then
    dump()
    dumped = true
    print("PDUMP|done")
    emu.stop(0)
  end

  if frame >= FRAMES then
    print(string.format("PDUMP|timeout frame=%d in_level=%s", frame, tostring(in_level_frame >= 0)))
    emu.stop(0)
  end
end, emu.eventType.startFrame)

print("PDUMP|armed")
