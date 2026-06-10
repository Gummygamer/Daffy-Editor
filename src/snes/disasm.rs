//! A linear 65816 disassembler.
//!
//! Built to read the graphics decompressor at `$82:8549-$82:8655` (the #1
//! reverse-engineering lead; see docs/reverse-engineering/graphics-pipeline.md)
//! and any other routine without leaving the project. It is a *linear sweep*
//! decoder: it walks instructions from a start address, formatting each. It
//! does not follow branches or build a control-flow graph.
//!
//! The one piece of CPU state a 65816 disassembler cannot avoid is the
//! accumulator/index width (the `M`/`X` status bits): immediate operands are
//! one or two bytes depending on them. We track this the only way a static
//! tool can — by honouring `REP`/`SEP` as we sweep, starting from a width the
//! caller supplies. `XCE` (which can flip back to 8-bit via emulation mode) is
//! not modelled; if a routine uses it, pass the correct starting widths and
//! disassemble the halves separately.
//!
//! Confidence: the opcode table is **confirmed** (the 65816 instruction set is
//! hardware-defined); any *interpretation* of a specific routine's bytes is a
//! game finding and labeled where it is recorded.

use crate::snes::lorom;

/// Addressing modes, grouped by the operand bytes they consume. `ImmM`/`ImmX`
/// are the only width-dependent ones (1 byte when the relevant flag is set to
/// 8-bit, 2 bytes when clear / 16-bit).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Implied,
    Accumulator,
    ImmM,
    ImmX,
    Imm8,
    Dp,
    DpX,
    DpY,
    DpInd,
    DpIndX,
    DpIndY,
    DpIndLong,
    DpIndLongY,
    Abs,
    AbsX,
    AbsY,
    AbsInd,
    AbsIndX,
    AbsIndLong,
    Long,
    LongX,
    StackRel,
    StackRelIndY,
    Rel8,
    Rel16,
    BlockMove,
}

impl Mode {
    /// Operand byte count, given the current accumulator (`m8`) and index
    /// (`x8`) widths (true = 8-bit).
    fn operand_len(self, m8: bool, x8: bool) -> usize {
        use Mode::*;
        match self {
            Implied | Accumulator => 0,
            ImmM => {
                if m8 {
                    1
                } else {
                    2
                }
            }
            ImmX => {
                if x8 {
                    1
                } else {
                    2
                }
            }
            Imm8 | Dp | DpX | DpY | DpInd | DpIndX | DpIndY | DpIndLong | DpIndLongY | StackRel
            | StackRelIndY | Rel8 => 1,
            Abs | AbsX | AbsY | AbsInd | AbsIndX | AbsIndLong | Rel16 | BlockMove => 2,
            Long | LongX => 3,
        }
    }
}

/// One decoded instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// SNES address of the opcode byte.
    pub addr: u32,
    /// The opcode byte plus its operand bytes.
    pub bytes: Vec<u8>,
    pub mnemonic: &'static str,
    /// Formatted operand (e.g. `#$C000`, `$1F2A,X`, `[$E7]`), empty for implied.
    pub operand: String,
    /// Accumulator/index widths in effect *for this instruction* (true=8-bit).
    pub m8: bool,
    pub x8: bool,
}

impl Instruction {
    /// `$82:8549  A9 00 C0     LDA #$C000`
    pub fn listing(&self) -> String {
        let bank = (self.addr >> 16) as u8;
        let lo = (self.addr & 0xFFFF) as u16;
        let raw = self
            .bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        let text = if self.operand.is_empty() {
            self.mnemonic.to_string()
        } else {
            format!("{} {}", self.mnemonic, self.operand)
        };
        format!("${bank:02X}:{lo:04X}  {raw:<14} {text}")
    }
}

fn le(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    for (i, b) in bytes.iter().enumerate() {
        v |= (*b as u32) << (8 * i);
    }
    v
}

fn format_operand(mode: Mode, addr: u32, instr_len: usize, operand: &[u8]) -> String {
    use Mode::*;
    let v = le(operand);
    match mode {
        Implied => String::new(),
        Accumulator => "A".to_string(),
        ImmM | ImmX | Imm8 => {
            if operand.len() == 2 {
                format!("#${v:04X}")
            } else {
                format!("#${v:02X}")
            }
        }
        Dp => format!("${v:02X}"),
        DpX => format!("${v:02X},X"),
        DpY => format!("${v:02X},Y"),
        DpInd => format!("(${v:02X})"),
        DpIndX => format!("(${v:02X},X)"),
        DpIndY => format!("(${v:02X}),Y"),
        DpIndLong => format!("[${v:02X}]"),
        DpIndLongY => format!("[${v:02X}],Y"),
        Abs => format!("${v:04X}"),
        AbsX => format!("${v:04X},X"),
        AbsY => format!("${v:04X},Y"),
        AbsInd => format!("(${v:04X})"),
        AbsIndX => format!("(${v:04X},X)"),
        AbsIndLong => format!("[${v:04X}]"),
        Long => format!("${v:06X}"),
        LongX => format!("${v:06X},X"),
        StackRel => format!("${v:02X},S"),
        StackRelIndY => format!("(${v:02X},S),Y"),
        Rel8 => {
            let disp = operand[0] as i8 as i32;
            let target = branch_target(addr, instr_len, disp);
            format!("${target:04X}")
        }
        Rel16 => {
            let disp = v as u16 as i16 as i32;
            let target = branch_target(addr, instr_len, disp);
            format!("${target:04X}")
        }
        // Machine order is [dest bank, src bank]; assembly lists src first.
        BlockMove => format!("${:02X},${:02X}", operand[1], operand[0]),
    }
}

/// Branch target, wrapped within the 64 KiB bank (relative branches do not
/// cross banks on the 65816).
fn branch_target(addr: u32, instr_len: usize, disp: i32) -> u16 {
    let next = (addr & 0xFFFF) as i32 + instr_len as i32;
    (next + disp) as u16
}

/// Decode one instruction at `data[idx..]` whose opcode lives at SNES `addr`,
/// using current widths. Returns the instruction and the next widths (only
/// `REP`/`SEP` change them). Returns `None` if the operand runs past the slice.
fn decode_one(data: &[u8], idx: usize, addr: u32, m8: bool, x8: bool) -> Option<(Instruction, bool, bool)> {
    let op = *data.get(idx)?;
    let (mnemonic, mode) = OPCODES[op as usize];
    let n = mode.operand_len(m8, x8);
    if idx + 1 + n > data.len() {
        return None;
    }
    let operand = &data[idx + 1..idx + 1 + n];
    let instr_len = 1 + n;
    let listing_operand = format_operand(mode, addr, instr_len, operand);

    // REP/SEP toggle the width flags for subsequent instructions. Bit 0x20 = M
    // (accumulator), bit 0x10 = X (index). SEP sets bits (-> 8-bit), REP clears.
    let (mut nm8, mut nx8) = (m8, x8);
    if !operand.is_empty() {
        let bits = operand[0];
        if op == 0xE2 {
            if bits & 0x20 != 0 {
                nm8 = true;
            }
            if bits & 0x10 != 0 {
                nx8 = true;
            }
        } else if op == 0xC2 {
            if bits & 0x20 != 0 {
                nm8 = false;
            }
            if bits & 0x10 != 0 {
                nx8 = false;
            }
        }
    }

    let mut bytes = Vec::with_capacity(instr_len);
    bytes.push(op);
    bytes.extend_from_slice(operand);
    Some((
        Instruction {
            addr,
            bytes,
            mnemonic,
            operand: listing_operand,
            m8,
            x8,
        },
        nm8,
        nx8,
    ))
}

/// Linearly disassemble `count` bytes of `data` starting at file offset
/// `start_pc`, which corresponds to SNES address `start_addr`. `m8`/`x8` are
/// the initial accumulator/index widths (true = 8-bit).
pub fn disassemble(data: &[u8], start_pc: usize, start_addr: u32, count: usize, mut m8: bool, mut x8: bool) -> Vec<Instruction> {
    let mut out = Vec::new();
    let end = (start_pc + count).min(data.len());
    let mut idx = start_pc;
    while idx < end {
        let addr = start_addr + (idx - start_pc) as u32;
        let Some((instr, nm8, nx8)) = decode_one(data, idx, addr, m8, x8) else {
            break;
        };
        idx += instr.bytes.len();
        m8 = nm8;
        x8 = nx8;
        out.push(instr);
    }
    out
}

/// Convenience: disassemble a SNES address range `[start_addr, end_addr]`
/// inclusive of the opcode at `end_addr`, resolving offsets via LoROM.
pub fn disassemble_snes_range(data: &[u8], start_addr: u32, end_addr: u32, m8: bool, x8: bool) -> Vec<Instruction> {
    let Ok(start_pc) = lorom::snes_to_pc(start_addr) else {
        return Vec::new();
    };
    // +1 so an opcode that begins exactly at end_addr is still decoded; the
    // sweep itself reads however many operand bytes that final opcode needs.
    let span = (end_addr.saturating_sub(start_addr) as usize) + 1;
    disassemble(data, start_pc, start_addr, span, m8, x8)
}

/// 256-entry opcode table: `(mnemonic, addressing mode)`, indexed by opcode.
#[rustfmt::skip]
static OPCODES: [(&str, Mode); 256] = {
    use Mode::*;
    [
        // 0x00
        ("BRK", Imm8), ("ORA", DpIndX), ("COP", Imm8), ("ORA", StackRel),
        ("TSB", Dp), ("ORA", Dp), ("ASL", Dp), ("ORA", DpIndLong),
        ("PHP", Implied), ("ORA", ImmM), ("ASL", Accumulator), ("PHD", Implied),
        ("TSB", Abs), ("ORA", Abs), ("ASL", Abs), ("ORA", Long),
        // 0x10
        ("BPL", Rel8), ("ORA", DpIndY), ("ORA", DpInd), ("ORA", StackRelIndY),
        ("TRB", Dp), ("ORA", DpX), ("ASL", DpX), ("ORA", DpIndLongY),
        ("CLC", Implied), ("ORA", AbsY), ("INC", Accumulator), ("TCS", Implied),
        ("TRB", Abs), ("ORA", AbsX), ("ASL", AbsX), ("ORA", LongX),
        // 0x20
        ("JSR", Abs), ("AND", DpIndX), ("JSL", Long), ("AND", StackRel),
        ("BIT", Dp), ("AND", Dp), ("ROL", Dp), ("AND", DpIndLong),
        ("PLP", Implied), ("AND", ImmM), ("ROL", Accumulator), ("PLD", Implied),
        ("BIT", Abs), ("AND", Abs), ("ROL", Abs), ("AND", Long),
        // 0x30
        ("BMI", Rel8), ("AND", DpIndY), ("AND", DpInd), ("AND", StackRelIndY),
        ("BIT", DpX), ("AND", DpX), ("ROL", DpX), ("AND", DpIndLongY),
        ("SEC", Implied), ("AND", AbsY), ("DEC", Accumulator), ("TSC", Implied),
        ("BIT", AbsX), ("AND", AbsX), ("ROL", AbsX), ("AND", LongX),
        // 0x40
        ("RTI", Implied), ("EOR", DpIndX), ("WDM", Imm8), ("EOR", StackRel),
        ("MVP", BlockMove), ("EOR", Dp), ("LSR", Dp), ("EOR", DpIndLong),
        ("PHA", Implied), ("EOR", ImmM), ("LSR", Accumulator), ("PHK", Implied),
        ("JMP", Abs), ("EOR", Abs), ("LSR", Abs), ("EOR", Long),
        // 0x50
        ("BVC", Rel8), ("EOR", DpIndY), ("EOR", DpInd), ("EOR", StackRelIndY),
        ("MVN", BlockMove), ("EOR", DpX), ("LSR", DpX), ("EOR", DpIndLongY),
        ("CLI", Implied), ("EOR", AbsY), ("PHY", Implied), ("TCD", Implied),
        ("JML", Long), ("EOR", AbsX), ("LSR", AbsX), ("EOR", LongX),
        // 0x60
        ("RTS", Implied), ("ADC", DpIndX), ("PER", Rel16), ("ADC", StackRel),
        ("STZ", Dp), ("ADC", Dp), ("ROR", Dp), ("ADC", DpIndLong),
        ("PLA", Implied), ("ADC", ImmM), ("ROR", Accumulator), ("RTL", Implied),
        ("JMP", AbsInd), ("ADC", Abs), ("ROR", Abs), ("ADC", Long),
        // 0x70
        ("BVS", Rel8), ("ADC", DpIndY), ("ADC", DpInd), ("ADC", StackRelIndY),
        ("STZ", DpX), ("ADC", DpX), ("ROR", DpX), ("ADC", DpIndLongY),
        ("SEI", Implied), ("ADC", AbsY), ("PLY", Implied), ("TDC", Implied),
        ("JMP", AbsIndX), ("ADC", AbsX), ("ROR", AbsX), ("ADC", LongX),
        // 0x80
        ("BRA", Rel8), ("STA", DpIndX), ("BRL", Rel16), ("STA", StackRel),
        ("STY", Dp), ("STA", Dp), ("STX", Dp), ("STA", DpIndLong),
        ("DEY", Implied), ("BIT", ImmM), ("TXA", Implied), ("PHB", Implied),
        ("STY", Abs), ("STA", Abs), ("STX", Abs), ("STA", Long),
        // 0x90
        ("BCC", Rel8), ("STA", DpIndY), ("STA", DpInd), ("STA", StackRelIndY),
        ("STY", DpX), ("STA", DpX), ("STX", DpY), ("STA", DpIndLongY),
        ("TYA", Implied), ("STA", AbsY), ("TXS", Implied), ("TXY", Implied),
        ("STZ", Abs), ("STA", AbsX), ("STZ", AbsX), ("STA", LongX),
        // 0xA0
        ("LDY", ImmX), ("LDA", DpIndX), ("LDX", ImmX), ("LDA", StackRel),
        ("LDY", Dp), ("LDA", Dp), ("LDX", Dp), ("LDA", DpIndLong),
        ("TAY", Implied), ("LDA", ImmM), ("TAX", Implied), ("PLB", Implied),
        ("LDY", Abs), ("LDA", Abs), ("LDX", Abs), ("LDA", Long),
        // 0xB0
        ("BCS", Rel8), ("LDA", DpIndY), ("LDA", DpInd), ("LDA", StackRelIndY),
        ("LDY", DpX), ("LDA", DpX), ("LDX", DpY), ("LDA", DpIndLongY),
        ("CLV", Implied), ("LDA", AbsY), ("TSX", Implied), ("TYX", Implied),
        ("LDY", AbsX), ("LDA", AbsX), ("LDX", AbsY), ("LDA", LongX),
        // 0xC0
        ("CPY", ImmX), ("CMP", DpIndX), ("REP", Imm8), ("CMP", StackRel),
        ("CPY", Dp), ("CMP", Dp), ("DEC", Dp), ("CMP", DpIndLong),
        ("INY", Implied), ("CMP", ImmM), ("DEX", Implied), ("WAI", Implied),
        ("CPY", Abs), ("CMP", Abs), ("DEC", Abs), ("CMP", Long),
        // 0xD0
        ("BNE", Rel8), ("CMP", DpIndY), ("CMP", DpInd), ("CMP", StackRelIndY),
        ("PEI", DpInd), ("CMP", DpX), ("DEC", DpX), ("CMP", DpIndLongY),
        ("CLD", Implied), ("CMP", AbsY), ("PHX", Implied), ("STP", Implied),
        ("JML", AbsIndLong), ("CMP", AbsX), ("DEC", AbsX), ("CMP", LongX),
        // 0xE0
        ("CPX", ImmX), ("SBC", DpIndX), ("SEP", Imm8), ("SBC", StackRel),
        ("CPX", Dp), ("SBC", Dp), ("INC", Dp), ("SBC", DpIndLong),
        ("INX", Implied), ("SBC", ImmM), ("NOP", Implied), ("XBA", Implied),
        ("CPX", Abs), ("SBC", Abs), ("INC", Abs), ("SBC", Long),
        // 0xF0
        ("BEQ", Rel8), ("SBC", DpIndY), ("SBC", DpInd), ("SBC", StackRelIndY),
        ("PEA", Abs), ("SBC", DpX), ("INC", DpX), ("SBC", DpIndLongY),
        ("SED", Implied), ("SBC", AbsY), ("PLX", Implied), ("XCE", Implied),
        ("JSR", AbsIndX), ("SBC", AbsX), ("INC", AbsX), ("SBC", LongX),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::lorom::BANK_SIZE;

    fn dis(bytes: &[u8], m8: bool, x8: bool) -> Vec<Instruction> {
        // Pretend the bytes live at $82:8549 (PC 0x10549).
        disassemble(bytes, 0, 0x82_8549, bytes.len(), m8, x8)
    }

    #[test]
    fn opcode_table_is_complete() {
        // Every entry must be populated (the array literal guarantees 256).
        assert_eq!(OPCODES.len(), 256);
        // Spot-check a few known opcodes.
        assert_eq!(OPCODES[0xA9], ("LDA", Mode::ImmM));
        assert_eq!(OPCODES[0xA2], ("LDX", Mode::ImmX));
        assert_eq!(OPCODES[0x8D], ("STA", Mode::Abs));
        assert_eq!(OPCODES[0x60], ("RTS", Mode::Implied));
        assert_eq!(OPCODES[0x54], ("MVN", Mode::BlockMove));
    }

    #[test]
    fn immediate_width_follows_m_flag() {
        // LDA #$xx with 8-bit A, then the same bytes with 16-bit A.
        let i8 = dis(&[0xA9, 0x12], true, true);
        assert_eq!(i8[0].mnemonic, "LDA");
        assert_eq!(i8[0].operand, "#$12");
        assert_eq!(i8[0].bytes.len(), 2);

        let i16 = dis(&[0xA9, 0x34, 0x12], false, true);
        assert_eq!(i16[0].operand, "#$1234");
        assert_eq!(i16[0].bytes.len(), 3);
    }

    #[test]
    fn immediate_width_follows_x_flag_independently() {
        // LDX # depends on X, not M.
        let i = dis(&[0xA2, 0x34, 0x12], true, false);
        assert_eq!(i[0].mnemonic, "LDX");
        assert_eq!(i[0].operand, "#$1234");
    }

    #[test]
    fn rep_sep_toggle_widths_mid_stream() {
        // REP #$30 (16-bit A+X) ; LDA #$1234 ; SEP #$20 (8-bit A) ; LDA #$56
        let code = [0xC2, 0x30, 0xA9, 0x34, 0x12, 0xE2, 0x20, 0xA9, 0x56];
        let i = dis(&code, true, true);
        assert_eq!(i[0].mnemonic, "REP");
        assert_eq!(i[0].operand, "#$30");
        assert_eq!(i[1].operand, "#$1234"); // now 16-bit
        assert_eq!(i[2].mnemonic, "SEP");
        assert_eq!(i[3].operand, "#$56"); // back to 8-bit
    }

    #[test]
    fn long_and_indirect_long_modes_format() {
        let i = dis(&[0xAF, 0x00, 0x80, 0x92], true, true); // LDA $928000
        assert_eq!(i[0].operand, "$928000");
        let j = dis(&[0xA7, 0xE7], true, true); // LDA [$E7]
        assert_eq!(j[0].operand, "[$E7]");
        let k = dis(&[0xB7, 0xE7], true, true); // LDA [$E7],Y
        assert_eq!(k[0].operand, "[$E7],Y");
    }

    #[test]
    fn relative_branch_target_is_bank_local() {
        // At $82:8549: BEQ +4 -> next instr is $854B, +4 = $854F.
        let i = dis(&[0xF0, 0x04], true, true);
        assert_eq!(i[0].mnemonic, "BEQ");
        assert_eq!(i[0].operand, "$854F");

        // Backward branch wraps correctly. BRA -2 (loop on self).
        let j = dis(&[0x80, 0xFE], true, true);
        assert_eq!(j[0].operand, "$8549");
    }

    #[test]
    fn block_move_lists_source_then_dest() {
        // MVN dest=$7F, src=$92  (machine bytes: 54 7F 92)
        let i = dis(&[0x54, 0x7F, 0x92], true, true);
        assert_eq!(i[0].mnemonic, "MVN");
        assert_eq!(i[0].operand, "$92,$7F");
    }

    #[test]
    fn listing_is_formatted() {
        let i = dis(&[0xA9, 0x00, 0xC0], false, true);
        assert_eq!(i[0].listing(), "$82:8549  A9 00 C0       LDA #$C000");
    }

    #[test]
    fn truncated_operand_stops_cleanly() {
        // LDA abs needs 2 operand bytes but only 1 is present.
        let i = dis(&[0xAD, 0x00], true, true);
        assert!(i.is_empty());
    }

    #[test]
    fn snes_range_helper_decodes_inclusive_end() {
        // NOP padding; ask for $828000..=$828002 -> 3 NOPs.
        let mut data = vec![0xEA; BANK_SIZE * 3];
        // ensure the bytes at PC for $828000 are NOP (they are, 0xEA fill)
        let _ = &mut data;
        let instrs = disassemble_snes_range(&data, 0x82_8000, 0x82_8002, true, true);
        assert_eq!(instrs.len(), 3);
        assert!(instrs.iter().all(|i| i.mnemonic == "NOP"));
        assert_eq!(instrs[0].addr, 0x82_8000);
        assert_eq!(instrs[2].addr, 0x82_8002);
    }
}
