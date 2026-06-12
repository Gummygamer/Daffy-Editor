-- trace_pal_loads.lua — log every graphics-loader call (id + descriptor target)
-- in execution order while driving input from boot into gameplay, AND dump the
-- resulting CGRAM at the same settle point. This gives a consistent picture: the
-- full ordered set of palette uploads and the CGRAM they produce, so we can see
-- which uploads survive into what the player sees (including loads issued by
-- routines the per-scene static scan in src/level/loader.rs doesn't cover).
--
-- Hooks the loader entry $82:84FD; Y = id*8 indexes the 8-byte descriptor table
-- at $82:8000 (mode, source24, params). Ids/addresses only — safe to commit.
--
--   MESEN_BIN=... ./run-headless.sh <rom> trace_pal_loads.lua 240 | grep '^GLOAD'

local ENTRY    = 0x8284FD
local OBJ_ITER = 0x80E9A8
local TABLE    = 0x828000          -- descriptor table, $82:8000
local FRAMES   = tonumber(FRAMES) or 6000
local SETTLE   = tonumber(SETTLE) or 180

local function r8(a)  return emu.read(a & 0xFFFF, emu.memType.snesMemory) end
local function r16(a) return r8(a) | (r8(a + 1) << 8) end
local function rom8(a) return emu.read(a, emu.memType.snesMemory) end

local frame = 0
local in_level_frame = -1
local ncalls = 0
local dumped = false

emu.addMemoryCallback(function()
  if in_level_frame < 0 then
    in_level_frame = frame
    print(string.format("GLOAD|IN-LEVEL frame=%d level=%d call#=%d", frame, r16(0x1EEA), ncalls))
  end
end, emu.callbackType.exec, OBJ_ITER, OBJ_ITER, emu.cpuType.snes)

emu.addMemoryCallback(function()
  local st = emu.getState()
  local y = st["cpu.y"] or 0
  local id = y // 8
  local rec = TABLE + id * 8
  local mode = rom8(rec)
  local src = rom8(rec + 1) | (rom8(rec + 2) << 8) | (rom8(rec + 3) << 16)
  local p0 = rom8(rec + 4)
  local p1 = rom8(rec + 5)
  local sz = rom8(rec + 6) | (rom8(rec + 7) << 8)
  ncalls = ncalls + 1
  local tgt
  if mode == 0 then
    tgt = string.format("VRAM word $%04X size $%04X", p0 | (p1 << 8), sz)
  elseif mode == 1 then
    tgt = string.format("CGRAM $%02X size $%04X", p0, sz)
  elseif mode == 2 then
    tgt = string.format("WRAM $%02X%02X%02X", rom8(rec + 6), p1, p0)
  else
    tgt = string.format("mode %d?", mode)
  end
  local tag = (in_level_frame >= 0) and "POST" or "pre "
  print(string.format("GLOAD|%s call#=%d frame=%d id=%d mode=%d src=$%06X %s",
    tag, ncalls, frame, id, mode, src, tgt))
end, emu.callbackType.exec, ENTRY, ENTRY, emu.cpuType.snes)

local function pulsing(period) return (frame % period) < (period // 2) end

local function hex_run(memType, off, len)
  local parts = {}
  for i = 0, len - 1 do parts[#parts + 1] = string.format("%02X", emu.read(off + i, memType)) end
  return table.concat(parts)
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
    print(string.format("GLOAD|cgram %s", hex_run(emu.memType.snesCgRam, 0, 512)))
    print(string.format("GLOAD|done frame=%d total_calls=%d", frame, ncalls))
    dumped = true
    emu.stop(0)
  end
  if frame >= FRAMES then
    print(string.format("GLOAD|timeout frame=%d total_calls=%d in_level=%s", frame, ncalls, tostring(in_level_frame >= 0)))
    emu.stop(0)
  end
end, emu.eventType.startFrame)

print("GLOAD|armed")
