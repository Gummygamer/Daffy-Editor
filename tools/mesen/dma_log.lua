-- dma_log.lua — live DMA transfer logger for Mesen2 (SNES)
--
-- Captures the ground truth static scanning cannot reach: it hooks the real
-- MDMAEN trigger ($420B) and reads the channel registers as the CPU set them
-- up, recovering EVERY transfer's true source -> dest -> size plus the trigger
-- PC (which points back at the upload routine).
--
-- Works under Mesen's headless test runner AND in the GUI:
--   Mesen --testRunner <rom> tools/mesen/dma_log.lua   # prints to stdout
--   Mesen <rom> tools/mesen/dma_log.lua                # logs to Script Window
-- For headless capture, filter stdout for the "DMACAP" prefix:
--   Mesen --testRunner rom.smc dma_log.lua 2>/dev/null | grep '^DMACAP'
--
-- API notes (Mesen 2.x, verified against a from-source build):
--   * the callback-type enum is emu.callbackType (NOT memCallbackType);
--   * emu.getState() returns a FLAT table keyed "cpu.pc" / "cpu.k" (no nesting);
--   * Lua io.* corrupts the heap under --testRunner — do not use it; emit with
--     print(), which is the only channel that reaches stdout headless.
--
-- Output lines carry addresses/sizes/PCs only — never ROM bytes — so captures
-- are safe to commit under docs/reverse-engineering/reports/.

local MDMAEN = 0x420B
local FRAME_CAP = tonumber(FRAME_CAP) or 1500   -- stop after N frames (~25s)
local seen, count = {}, 0

local function dest_kind(b)
  if b == 0x18 or b == 0x19 then return "vram"
  elseif b == 0x22 then return "cgram"
  elseif b == 0x04 then return "oam"
  elseif b == 0x80 then return "wram"
  else return string.format("r%02X", b) end
end

-- $7E/$7F = WRAM; low pages of system banks = LowRAM; otherwise ROM.
local function space(bank, addr)
  if bank == 0x7E or bank == 0x7F then return "WRAM"
  elseif (bank <= 0x3F or (bank >= 0x80 and bank <= 0xBF)) and addr < 0x2000 then return "LowRAM"
  else return "ROM" end
end

local function emit(line)
  print("DMACAP|" .. line)   -- stdout (headless)
  emu.log(line)              -- Script Window (GUI)
end

local function on_mdmaen(addr, value)
  if value == 0 then return end                 -- no channels selected
  local st = emu.getState()
  local pc = ((st["cpu.k"] or 0) << 16) | (st["cpu.pc"] or 0)
  for ch = 0, 7 do
    if (value & (1 << ch)) ~= 0 then
      local base = 0x4300 + ch * 0x10
      local dmap = emu.read(base + 0, emu.memType.snesMemory)
      local bbad = emu.read(base + 1, emu.memType.snesMemory)
      local a1t  = emu.read16(base + 2, emu.memType.snesMemory)
      local a1b  = emu.read(base + 4, emu.memType.snesMemory)
      local das  = emu.read16(base + 5, emu.memType.snesMemory)
      local size = das == 0 and 0x10000 or das  -- DAS of 0 transfers 65536
      local dir  = (dmap & 0x80) ~= 0 and "B2A" or "A2B"  -- A2B = upload
      local key = string.format("%06X:%d:%02X%04X:%02X:%04X", pc, ch, a1b, a1t, bbad, size)
      if not seen[key] then
        seen[key] = true
        count = count + 1
        emit(string.format("%d|ch%d|pc=%06X|%s|src=%02X:%04X|%s|size=%d|%s",
          count, ch, pc, dest_kind(bbad), a1b, a1t, space(a1b, a1t), size, dir))
      end
    end
  end
end

emu.addMemoryCallback(on_mdmaen, emu.callbackType.write, MDMAEN, MDMAEN, emu.cpuType.snes)
emit("armed (logging unique DMA transfers via $420B)")

local n = 0
emu.addEventCallback(function()
  n = n + 1
  if n >= FRAME_CAP then
    emit("done frames=" .. n .. " unique=" .. count)
    emu.stop(0)
  end
end, emu.eventType.startFrame)
