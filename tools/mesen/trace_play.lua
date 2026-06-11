-- trace_play.lua — drive controller input to reach gameplay, then report the
-- level number and confirm we are in a level.
--
-- The title/intro flow is unknown, so we pulse START (and occasionally A / RIGHT)
-- until the in-level object iterator $80:E9A8 starts executing each frame. We log
-- the live level number $1EEA and the per-scene data pointers when that happens.
-- This is the bring-up step for trace_fields.lua (which read-watches the level's
-- object/attr/map regions once in gameplay).
--
--   MESEN_BIN=... ./run-headless.sh <rom> trace_play.lua 240 | grep '^PLAY'

local OBJ_ITER = 0x80E9A8        -- in-level object iterator (proof of gameplay)
local FRAMES   = tonumber(FRAMES) or 4000

local function r8(a)  return emu.read(a & 0xFFFF, emu.memType.snesMemory) end
local function r16(a) return r8(a) | (r8(a + 1) << 8) end

local frame = 0
local in_level = false
local iter_hits = 0
local last_level = -1

-- Detect gameplay: the object iterator runs once we are in a level.
emu.addMemoryCallback(function()
  iter_hits = iter_hits + 1
  if not in_level then
    in_level = true
    print(string.format("PLAY|IN-LEVEL at frame %d  level($1EEA)=%d", frame, r16(0x1EEA)))
    print(string.format("PLAY|   D3=%04X D5=%04X D9=%04X DB=%04X DD=%04X DF=%04X  1EF4=%04X 1EF8=%04X",
      r16(0xD3), r16(0xD5), r16(0xD9), r16(0xDB), r16(0xDD), r16(0xDF), r16(0x1EF4), r16(0x1EF8)))
  end
end, emu.callbackType.exec, OBJ_ITER, OBJ_ITER, emu.cpuType.snes)

-- Press a button for the first half of each `period`-frame cycle.
local function pulsing(period) return (frame % period) < (period // 2) end

emu.addEventCallback(function()
  frame = frame + 1

  -- Input plan: let logos pass, then pulse START; once past the title, also
  -- nudge A / RIGHT to get through any intro/map screen into the level.
  local inp = {}
  if frame > 120 then
    if pulsing(24) then inp.start = true end
    if frame > 600 then
      if pulsing(32) then inp.a = true end
      if pulsing(40) then inp.right = true end
    end
  end
  emu.setInput(0, inp)

  local lv = r16(0x1EEA)
  if lv ~= last_level then
    print(string.format("PLAY|frame %d  $1EEA -> %d", frame, lv))
    last_level = lv
  end

  if frame >= FRAMES then
    print(string.format("PLAY|done frame=%d in_level=%s iter_hits=%d level=%d",
      frame, tostring(in_level), iter_hits, r16(0x1EEA)))
    emu.stop(0)
  end
end, emu.eventType.startFrame)

print("PLAY|armed; driving input")
