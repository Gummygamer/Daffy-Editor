-- trace_gfx_dispatcher.lua — find the real caller of the gfx loader.
--
-- The loader preamble $82:84F8 does PHP / REP#$30 / PHX / PHY (1+2+2 = 5 bytes
-- pushed) then falls through to the decompressor entry $82:84FD. So at $82:84FD
-- the stack top holds the pushed Y (sp+1,2), X (sp+3,4), P (sp+5); the *real*
-- return address of a `JSL $82:84F8` caller sits below them at sp+6,7,8.
-- (That is why trace_gfx_loader's `ret` was garbage: sp+1..3 = Ylo,Yhi,Xlo.)
--
-- This hook reads the genuine return address at sp+6..8 plus X/Y, so we can see
-- the dispatcher that selects each gfx id. Addresses/registers only — safe to
-- commit a report.
--   MESEN_BIN=... ./run-headless.sh <rom> trace_gfx_dispatcher.lua 90 | grep '^GDISP'

local ENTRY = 0x8284FD
local CALL_CAP = tonumber(CALL_CAP) or 64

local function rd(a) return emu.read(a & 0xFFFF, emu.memType.snesMemory) end

local seen = {}
local ncalls = 0

emu.addMemoryCallback(function()
  local st = emu.getState()
  local sp = st["cpu.sp"] or 0
  -- real JSL return = pushed P(1)+X(2)+Y(2) above it, so return is at sp+6..8.
  local rlo  = rd(sp + 6)
  local rhi  = rd(sp + 7)
  local rbk  = rd(sp + 8)
  local ret  = (rbk << 16) | (rhi << 8) | rlo
  -- JSL pushes (return_addr - 1); the calling instruction starts 3 bytes before.
  local callsite = (ret - 3) & 0xFFFFFF
  local x = st["cpu.x"] or 0
  local y = st["cpu.y"] or 0
  ncalls = ncalls + 1
  local key = callsite
  seen[key] = (seen[key] or 0) + 1
  if seen[key] <= 3 then
    print(string.format("GDISP|call#=%d ret=%06X callsite=%06X X=%04X Y=%04X id=%d",
      ncalls, ret, callsite, x, y, (y // 8)))
  end
  if ncalls >= CALL_CAP then print("GDISP|call-cap reached"); emu.stop(0) end
end, emu.callbackType.exec, ENTRY, ENTRY, emu.cpuType.snes)

print("GDISP|armed entry=$82:84FD (reads sp+6..8)")

local n = 0
emu.addEventCallback(function()
  n = n + 1
  if n >= 4000 then
    print(string.format("GDISP|frame-cap reached, %d calls", ncalls))
    for cs, c in pairs(seen) do print(string.format("GDISP|callsite %06X count=%d", cs, c)) end
    emu.stop(0)
  end
end, emu.eventType.startFrame)
