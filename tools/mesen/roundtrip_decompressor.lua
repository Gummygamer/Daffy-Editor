-- roundtrip_decompressor.lua — capture ground truth for the graphics codec.
--
-- Hooks the decompressor entry ($82:84FD) to read its 24-bit source pointer
-- (DP $16/$17/$18) and destination pointer (DP $19/$1A/$1B), then hooks the
-- routine's RTL ($82:8577) to dump the bytes it wrote into the WRAM tile
-- staging area. Feeding the source ROM bytes through the Rust decoder and
-- comparing to this dump is a full, real-data round-trip (see tools/roundtrip.sh).
--
-- We capture the FIRST call whose source lands in the known compressed-graphics
-- banks $92/$93/$95/$96, then stop.
--
-- IMPORTANT: this dump DOES contain decoded ROM graphics bytes, so its output
-- is for LOCAL verification only and must never be committed (tools/roundtrip.sh
-- writes it to a temp file). The lua itself emits no bytes when committed.
--
-- API notes (Mesen 2.x): emu.callbackType.exec; emu.getState() flat keys
-- "cpu.k"/"cpu.pc"/"cpu.d"; emu.read(addr, emu.memType.snesMemory) on the CPU
-- bus; print() is the only headless output channel (io.* corrupts the heap).
--
--   Mesen --testRunner rom.smc roundtrip_decompressor.lua 2>/dev/null | grep '^STRACE'

local ENTRY = 0x8284FD
local RTL = 0x828577
local DUMP_LEN = tonumber(DUMP_LEN) or 0x2000 -- staging area (decoded output can exceed $1000)
local GFX_BANKS = { [0x92] = true, [0x93] = true, [0x95] = true, [0x96] = true }

local armed = false   -- saw a qualifying entry, waiting for its RTL
local done = false
local src_snes, dst_snes

local function rd(addr)
  return emu.read(addr, emu.memType.snesMemory)
end

local function ptr24(d, lo)
  return (rd(d + lo + 2) << 16) | (rd(d + lo + 1) << 8) | rd(d + lo)
end

emu.addMemoryCallback(function()
  if done or armed then return end
  local st = emu.getState()
  local d = st["cpu.d"] or 0
  local s = ptr24(d, 0x16)
  if not GFX_BANKS[(s >> 16) & 0xFF] then return end
  src_snes = s
  dst_snes = ptr24(d, 0x19)
  armed = true
  print(string.format("STRACE|entry src=%06X dst=%06X d=%04X", src_snes, dst_snes, d))
end, emu.callbackType.exec, ENTRY, ENTRY, emu.cpuType.snes)

emu.addMemoryCallback(function()
  if done or not armed then return end
  done = true
  print(string.format("STRACE|src=%06X", src_snes))
  print(string.format("STRACE|dst=%06X len=%04X", dst_snes, DUMP_LEN))
  -- Emit the staging bytes 32 per line as hex.
  local line = {}
  for i = 0, DUMP_LEN - 1 do
    line[#line + 1] = string.format("%02X", rd(dst_snes + i))
    if #line == 32 then
      print("STRACE|dump " .. table.concat(line))
      line = {}
    end
  end
  if #line > 0 then print("STRACE|dump " .. table.concat(line)) end
  print("STRACE|end")
  emu.stop(0)
end, emu.callbackType.exec, RTL, RTL, emu.cpuType.snes)

print("STRACE|armed entry=$82:84FD rtl=$82:8577")

-- Safety stop in case the qualifying call never happens.
local FRAME_CAP = tonumber(FRAME_CAP) or 1500
local n = 0
emu.addEventCallback(function()
  n = n + 1
  if n >= FRAME_CAP and not done then
    print("STRACE|timeout no qualifying decompress call seen")
    emu.stop(0)
  end
end, emu.eventType.startFrame)
