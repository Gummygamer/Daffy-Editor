-- trace_fields.lua — decode the level's record layouts by read-watching them.
--
-- Drives input (like trace_play.lua) to reach level 0, then sets ROM read
-- callbacks on that level's three undecoded regions and records, per reader PC,
-- which byte offsets it touches:
--   * object/entity spawn list  $81:8DAE  (22-byte records -> offset % 22)
--   * per-metatile attribute map $88:A600
--   * tilemap                    $88:A86B  (80x24 16-bit cells)
-- The reader PCs + the offsets they hit reveal the field layout. We drive RIGHT
-- to scroll the level so the column renderer streams map/attr data.
--
-- Emits PCs/offsets only (no ROM bytes) -> safe to commit a report.
--   MESEN_BIN=... ./run-headless.sh <rom> trace_fields.lua 300 | grep '^FIELD'

local OBJ_BASE  = 0x818DAE
local OBJ_LEN   = 0x16 * 16          -- first 16 records
local OBJ_REC   = 0x16
local ATTR_BASE = 0x88A600
local ATTR_LEN  = 0x300
local MAP_BASE  = 0x88A86B
local MAP_LEN   = 80 * 24 * 2
local FRAMES    = tonumber(FRAMES) or 6000

local function r16(a)
  return emu.read(a & 0xFFFF, emu.memType.snesMemory)
       | (emu.read((a + 1) & 0xFFFF, emu.memType.snesMemory) << 8)
end
local function pc()
  local st = emu.getState()
  return ((st["cpu.k"] or 0) << 16) | (st["cpu.pc"] or 0)
end

local frame = 0
local armed = false

-- region name -> { pc -> {count=n, offs={offset=true,...}} }
local hits = { obj = {}, attr = {}, map = {} }

local function record(region, base, addr)
  local off = (addr - base) & 0xFFFFFF
  local p = pc()
  local t = hits[region][p]
  if not t then t = { count = 0, offs = {} }; hits[region][p] = t end
  t.count = t.count + 1
  if region == "obj" then off = off % OBJ_REC end
  t.offs[off] = (t.offs[off] or 0) + 1
end

local function arm()
  emu.addMemoryCallback(function(addr) record("obj", OBJ_BASE, addr) end,
    emu.callbackType.read, OBJ_BASE, OBJ_BASE + OBJ_LEN - 1, emu.cpuType.snes)
  emu.addMemoryCallback(function(addr) record("attr", ATTR_BASE, addr) end,
    emu.callbackType.read, ATTR_BASE, ATTR_BASE + ATTR_LEN - 1, emu.cpuType.snes)
  emu.addMemoryCallback(function(addr) record("map", MAP_BASE, addr) end,
    emu.callbackType.read, MAP_BASE, MAP_BASE + MAP_LEN - 1, emu.cpuType.snes)
  print("FIELD|armed read-watches (obj/attr/map)")
end

local function dump()
  for _, region in ipairs({ "obj", "attr", "map" }) do
    -- sort reader PCs by hit count, show top few + the offsets they touch.
    local arr = {}
    for p, t in pairs(hits[region]) do arr[#arr + 1] = { p = p, t = t } end
    table.sort(arr, function(a, b) return a.t.count > b.t.count end)
    print(string.format("FIELD|== %s: %d reader PCs ==", region, #arr))
    for i = 1, math.min(8, #arr) do
      local e = arr[i]
      local offs = {}
      for o, _ in pairs(e.t.offs) do offs[#offs + 1] = o end
      table.sort(offs)
      local os = {}
      for _, o in ipairs(offs) do os[#os + 1] = string.format("%X", o) end
      print(string.format("FIELD|  %s reads=%d offs=[%s]",
        ("%06X"):format(e.p), e.t.count, table.concat(os, " ")))
    end
  end
end

emu.addEventCallback(function()
  frame = frame + 1
  local inp = {}
  if frame > 120 then
    if (frame % 24) < 12 then inp.start = true end
    if frame > 600 then
      if (frame % 32) < 16 then inp.a = true end
      inp.right = true                     -- hold RIGHT to scroll the level
    end
  end
  emu.setInput(0, inp)

  -- Arm once we are in level 0 with the expected pointers live.
  if not armed and r16(0x1EEA) == 0 and r16(0xD9) == 0xA86B and frame > 1500 then
    armed = true
    arm()
  end

  if frame >= FRAMES then
    print(string.format("FIELD|done frame=%d armed=%s", frame, tostring(armed)))
    dump()
    emu.stop(0)
  end
end, emu.eventType.startFrame)

print("FIELD|armed driver")
