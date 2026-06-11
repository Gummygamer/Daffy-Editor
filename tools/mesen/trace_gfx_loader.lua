-- trace_gfx_loader.lua — find the graphics-id -> source-pointer loader.
--
-- Hooks the decompressor entry ($82:84FD) and, for every call, records:
--   * the CALLER's return address read off the stack (the loader / dispatcher);
--   * the source pointer DP $16/$17/$18 and dest pointer DP $19/$1A/$1B it set up;
--   * the CPU registers A/X/Y and direct page D at entry (a gfx id / table index
--     is usually still live in one of these).
-- The decompressor has no immediate `JSL $82:84FD` anywhere in the ROM (verified
-- statically), so it is reached indirectly; the stack return address is the only
-- way to see who actually calls it and with what index.
--
-- Emits addresses/registers only (no ROM bytes) -> safe to commit a report.
--
--   MESEN_BIN=... ./run-headless.sh <rom> trace_gfx_loader.lua 2>/dev/null | grep '^GLOAD'
--
-- API notes (Mesen 2.x): emu.callbackType.exec; emu.getState() FLAT keys
-- "cpu.pc"/"cpu.k"/"cpu.sp"/"cpu.d"/"cpu.a"/"cpu.x"/"cpu.y"; the stack lives in
-- bank $00, so reading return bytes at (sp+n)&0xFFFF on snesMemory is bank $00;
-- print() is the only headless output channel.

local ENTRY = 0x8284FD
local FRAME_CAP = tonumber(FRAME_CAP) or 4000
local CALL_CAP = tonumber(CALL_CAP) or 64

local function rd(addr)
  return emu.read(addr & 0xFFFF, emu.memType.snesMemory)
end

local function ptr24(d, lo)
  return (rd(d + lo + 2) << 16) | (rd(d + lo + 1) << 8) | rd(d + lo)
end

local seen = {}        -- caller -> count, dedupe noisy repeats
local ncalls = 0

emu.addMemoryCallback(function()
  local st = emu.getState()
  local sp = st["cpu.sp"] or 0
  local d = st["cpu.d"] or 0
  -- Return address pushed by the (long) call: lo, hi, bank just above SP.
  local rlo = rd(sp + 1)
  local rhi = rd(sp + 2)
  local rbank = rd(sp + 3)
  local ret = (rbank << 16) | (rhi << 8) | rlo

  local src = ptr24(d, 0x16)
  local dst = ptr24(d, 0x19)
  local a = st["cpu.a"] or 0
  local x = st["cpu.x"] or 0
  local y = st["cpu.y"] or 0

  ncalls = ncalls + 1
  local key = ret
  seen[key] = (seen[key] or 0) + 1
  -- Log the first few hits per distinct caller; that is enough to read its index.
  if seen[key] <= 3 then
    print(string.format(
      "GLOAD|call#=%d ret=%06X src=%06X dst=%06X A=%04X X=%04X Y=%04X D=%04X",
      ncalls, ret, src, dst, a, x, y, d))
  end
  if ncalls >= CALL_CAP then
    print("GLOAD|call-cap reached")
    emu.stop(0)
  end
end, emu.callbackType.exec, ENTRY, ENTRY, emu.cpuType.snes)

print("GLOAD|armed entry=$82:84FD")

local n = 0
emu.addEventCallback(function()
  n = n + 1
  if n >= FRAME_CAP then
    print(string.format("GLOAD|frame-cap %d reached, %d calls total", FRAME_CAP, ncalls))
    -- summary of distinct callers
    for ret, c in pairs(seen) do
      print(string.format("GLOAD|caller %06X count=%d", ret, c))
    end
    emu.stop(0)
  end
end, emu.eventType.startFrame)
