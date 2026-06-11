-- trace_scene.lua — capture a scene/level setup's data pointers.
--
-- The scene-setup code (e.g. $81:800F) loads a gfx batch, fills a block of
-- direct-page / low-RAM pointers that describe the scene's data, then uploads
-- the sound bank via $80:99AD. We hook $80:99AD (hit once per scene load) and
-- dump the candidate level-data pointers live at that moment, so we can see what
-- ROM regions a real scene references. Addresses/registers only — safe to commit.
--
--   MESEN_BIN=... ./run-headless.sh <rom> trace_scene.lua 200 | grep '^SCENE'

local HOOK = 0x8099AD
local CAP  = tonumber(CAP) or 24

local function r8(a)  return emu.read(a & 0xFFFF, emu.memType.snesMemory) end
local function r16(a) return r8(a) | (r8(a + 1) << 8) end
-- a 24-bit pointer stored little-endian at DP `a` (lo, mid, bank)
local function r24(a) return r8(a) | (r8(a + 1) << 8) | (r8(a + 2) << 16) end

local n = 0
emu.addMemoryCallback(function()
  n = n + 1
  -- $00/$03 is the sound pointer passed to $80:99AD; the rest are the scene's
  -- data-pointer block set up just before it.
  print(string.format("SCENE|#%d snd=%02X:%04X", n, r8(0x03) & 0xFF, r16(0x00)))
  print(string.format("SCENE|   D3=%04X D5=%04X D7=%04X D9=%04X DB=%04X DD=%04X DF=%04X",
    r16(0xD3), r16(0xD5), r16(0xD7), r16(0xD9), r16(0xDB), r16(0xDD), r16(0xDF)))
  print(string.format("SCENE|   ptr_D3=%06X ptr_D7=%06X", r24(0xD3), r24(0xD7)))
  print(string.format("SCENE|   1EF4=%04X 1EF8=%04X 1EFA=%04X  (=$%02X:%04X / $%02X:%04X)",
    r16(0x1EF4), r16(0x1EF8), r16(0x1EFA),
    r16(0x1EF8) & 0xFF, r16(0x1EF4), r16(0x1EF8) & 0xFF, r16(0x1EFA)))
  if n >= CAP then print("SCENE|cap reached"); emu.stop(0) end
end, emu.callbackType.exec, HOOK, HOOK, emu.cpuType.snes)

print("SCENE|armed $80:99AD")

local f = 0
emu.addEventCallback(function()
  f = f + 1
  if f >= 12000 then print(string.format("SCENE|frame-cap, %d scene loads", n)); emu.stop(0) end
end, emu.eventType.startFrame)
