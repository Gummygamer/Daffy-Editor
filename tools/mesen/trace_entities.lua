-- trace_entities.lua — capture each object/enemy/item the game actually spawns.
--
-- Static analysis (src/bin/dump_entities.rs) cannot enumerate the spawn list:
-- the per-scene routine at the $80:E8D8/$E900 master table sets the list base
-- ($1EF4) and bank ($1EF8) imperatively, the record COUNT is a runtime active-
-- object counter (not a static field), and record byte $0E is a *row* coordinate
-- (fed to the width-multiply at $80:F1FD), NOT the type the loader assumes.
--
-- So we observe spawns live. The object activator at $80:E9A8 copies a 22-byte
-- record from ($1EF4)+index*$16 into the working block at $3B..$50, then assigns
-- a slot. We hook:
--   * $80:E9A8 (entry)  — log the list base/bank + current scene id ($1EEA).
--   * $80:E9CA (post-copy) — dump the full 22-byte record now sitting at $3B.
--
-- Play (or scroll) through a level so every object activates; cross-reference the
-- small-valued record fields against the sprite that appears on screen to pin the
-- *type* field and build the item-vs-enemy catalog.
--
-- This dump contains ROM-derived record bytes, so — like roundtrip_decompressor
-- — its output is LOCAL-ONLY and must NOT be committed.
--
-- It **auto-drives input** (like trace_play.lua): pulse START past the title,
-- then once the activator proves we are in a level, hold RIGHT and pulse A to
-- scroll/jump across the level so its objects activate. Manual play in the GUI
-- Script Window still works — auto-input is additive, not required.
--
--   MESEN_BIN=... ./run-headless.sh <rom> trace_entities.lua 1200 | grep '^ENT'
--
-- Knobs (pass as env-style globals, e.g. `FRAMES=20000 ...`):
--   FRAMES   total frames before stop          (default 20000)
--   NODRIVE  set to 1 to disable auto-input (manual play only)

local ENTRY    = 0x80E9A8 -- object activator entry
local POSTCOPY = 0x80E9CA -- just after the 22-byte record copy to $3B..$50
local REC      = 0x16      -- record stride (22 bytes), confirmed at $80:E9B5

local FRAMES  = tonumber(FRAMES) or 20000
local NODRIVE = tonumber(NODRIVE) == 1

local function r8(a)  return emu.read(a & 0xFFFF, emu.memType.snesMemory) end
local function r16(a) return r8(a) | (r8(a + 1) << 8) end
-- Full 24-bit CPU-bus read (for following the $1EF8:$1EF4 list pointer into ROM).
local function r8f(a)  return emu.read(a & 0xFFFFFF, emu.memType.snesMemory) end

-- Records to read straight off the list when we first enter a scene. Traversal-
-- based activation only yields object #$1EE8 (often 0), so we instead walk the
-- list directly from its live base; the human eyes where it turns to garbage.
local LIST_DUMP = tonumber(LISTDUMP) or 24
local dumped_scene = {}

-- Dump up to LIST_DUMP raw 22-byte records starting at the live list base.
local function dump_list(scene)
  if dumped_scene[scene] then return end
  dumped_scene[scene] = true
  local bank = r16(0x1EF8) & 0xFF
  local off  = r16(0x1EF4)
  local base = (bank << 16) | off
  print(string.format("ENT|LIST scene=%02X base=$%02X:%04X dumping %d records:",
    scene, bank, off, LIST_DUMP))
  for rec = 0, LIST_DUMP - 1 do
    local a = base + rec * REC
    local b = {}
    for i = 0, REC - 1 do b[i] = r8f(a + i) end
    local hex = {}
    for i = 0, REC - 1 do hex[#hex + 1] = string.format("%02X", b[i]) end
    print(string.format("ENT|  [%2d] %s  col=%d row=%d",
      rec, table.concat(hex, " "),
      b[0x0C] | (b[0x0D] << 8), b[0x0E] | (b[0x0F] << 8)))
  end
end

-- De-dupe: the same record can re-activate many times as it scrolls in/out.
local seen = {}
local n_unique = 0
local frame = 0
local in_level = false

-- Detecting gameplay (and switching the input plan to "traverse") keys off the
-- activator firing, exactly like trace_play.lua.
emu.addMemoryCallback(function()
  local scene = r8(0x1EEA)
  if not in_level then
    in_level = true
    print(string.format("ENT|IN-LEVEL frame=%d scene=%02X (DD=%d DF=%d)",
      frame, scene, r16(0xDD), r16(0xDF)))
  end
  -- Dump the whole list once per scene (does not need traversal).
  dump_list(scene)
  print(string.format("ENT|spawn scene=%02X list=$%02X:%04X idx=%d",
    r8(0x1EEA), r16(0x1EF8) & 0xFF, r16(0x1EF4), r16(0x1EE8)))
end, emu.callbackType.exec, ENTRY, ENTRY, emu.cpuType.snes)

emu.addMemoryCallback(function()
  -- $3B..$50 now hold the freshly-copied 22-byte record.
  local b = {}
  for i = 0, REC - 1 do b[i] = r8(0x3B + i) end
  -- key by scene + whole record so duplicates collapse but cross-level repeats
  -- of the same bytes still register once per scene.
  local key = string.format("%02X:", r8(0x1EEA)) .. table.concat(b, ",")
  if seen[key] then return end
  seen[key] = true
  n_unique = n_unique + 1

  local hex = {}
  for i = 0, REC - 1 do hex[#hex + 1] = string.format("%02X", b[i]) end
  -- known geometry: word $04 / $06 packed Y/X (speculative), $0C col, $0E row.
  print(string.format("ENT|rec %s  | col($0C)=%d row($0E)=%d w04=%04X w06=%04X",
    table.concat(hex, " "),
    b[0x0C] | (b[0x0D] << 8),
    b[0x0E] | (b[0x0F] << 8),
    b[0x04] | (b[0x05] << 8),
    b[0x06] | (b[0x07] << 8)))
end, emu.callbackType.exec, POSTCOPY, POSTCOPY, emu.cpuType.snes)

-- Hold a button for the first half of each `period`-frame cycle.
local function pulsing(period) return (frame % period) < (period // 2) end

emu.addEventCallback(function()
  frame = frame + 1

  if not NODRIVE then
    local inp = {}
    if not in_level then
      -- Menu/intro phase: let logos pass, then pulse START + nudge A/RIGHT to
      -- get through the title/mission-select into a level.
      if frame > 120 then
        if pulsing(24) then inp.start = true end
        if frame > 600 then
          if pulsing(32) then inp.a = true end
          if pulsing(40) then inp.right = true end
        end
      end
    else
      -- Traverse phase: hold RIGHT to scroll objects into the activation window,
      -- pulse A to hop gaps/obstacles. Briefly release RIGHT each cycle so a
      -- stuck jump can re-seat. (Auto-play is best-effort coverage, not a run.)
      if (frame % 64) < 56 then inp.right = true end
      if pulsing(28) then inp.a = true end
    end
    emu.setInput(0, inp)
  end

  if frame >= FRAMES then
    print(string.format("ENT|done frame=%d in_level=%s unique_records=%d",
      frame, tostring(in_level), n_unique))
    emu.stop(0)
  end
end, emu.eventType.startFrame)

print(string.format("ENT|armed $80:E9A8 + $80:E9CA  drive=%s frames=%d",
  tostring(not NODRIVE), FRAMES))
