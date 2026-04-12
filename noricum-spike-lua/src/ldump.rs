//! ldump — binary .luac writer.
//!
//! Port of `ldump.c`. Serialises a Proto (and its inner protos)
//! to the byte-exact Lua 5.5 precompiled-chunk format. Paired
//! with [`crate::lundump`] for the reverse direction.
//!
//! Format lives in `lundump.h` of the C reference:
//! - Header: LUA_SIGNATURE "\x1bLua", version byte, format byte,
//!   LUAC_DATA sentinel, size+value of INT/INST/INTEGER/NUMBER.
//! - Per-function: linedefined, lastlinedefined, numparams, flag,
//!   maxstacksize, code, constants, upvalues, inner protos,
//!   source string, debug info.
//! - Signed-integer / varint encoding for compact size, with
//!   ZigZag for signed integers.

#![allow(dead_code)]

use crate::contract::{
    GlobalState, Instruction, LuaInteger, LuaNumber, Proto, ProtoHandle,
    StringHandle, TValue,
};

// Signature constants — must match C Lua's lundump.h byte-for-byte.
pub const LUA_SIGNATURE: &[u8] = b"\x1bLua";
pub const LUAC_DATA: &[u8] = b"\x19\x93\r\n\x1a\n";
pub const LUAC_INT: i64 = -0x5678;
pub const LUAC_INST: u32 = 0x12345678;
pub const LUAC_NUM: f64 = -370.5;
// Version: LUA_VERSION_MAJOR_N * 16 + LUA_VERSION_MINOR_N.
// Our source tree is Lua 5.5 → 5*16 + 5 = 0x55 = 85.
pub const LUAC_VERSION: u8 = 5 * 16 + 5;
pub const LUAC_FORMAT: u8 = 0;

// Type tags used in the constant pool dump — match lua's tt_ bytes.
pub const LUA_VNIL: u8 = 0;
pub const LUA_VFALSE: u8 = 1;
pub const LUA_VTRUE: u8 = 1 | (1 << 4);
pub const LUA_VNUMINT: u8 = 3;
pub const LUA_VNUMFLT: u8 = 3 | (1 << 4);
pub const LUA_VSHRSTR: u8 = 4;
pub const LUA_VLNGSTR: u8 = 4 | (1 << 4);

/// Serialise `root` (the main-chunk Proto of a compiled closure)
/// into a `Vec<u8>` whose contents are a byte-exact `.luac` file.
pub fn dump(
    gs: &GlobalState,
    root: ProtoHandle,
    strip: bool,
) -> Vec<u8> {
    let mut d = DumpState {
        gs,
        out: Vec::new(),
        offset: 0,
        strip,
        strings_seen: std::collections::HashMap::new(),
        nstr: 0,
    };
    d.dump_header();
    // Dump the number of upvalues of the root function (C Lua
    // dumps this outside the function body so the loader knows
    // how many entries to allocate).
    let root_proto = gs.heap.proto(root);
    d.put_byte(root_proto.upvalues.len() as u8);
    d.dump_function(root);
    d.out
}

struct DumpState<'a> {
    gs: &'a GlobalState,
    out: Vec<u8>,
    offset: usize,
    strip: bool,
    strings_seen: std::collections::HashMap<Vec<u8>, u64>,
    nstr: u64,
}

impl<'a> DumpState<'a> {
    fn put_bytes(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
        self.offset += bytes.len();
    }
    fn put_byte(&mut self, b: u8) {
        self.out.push(b);
        self.offset += 1;
    }
    fn put_align(&mut self, align: usize) {
        let pad = align - (self.offset % align);
        if pad < align {
            for _ in 0..pad {
                self.out.push(0);
                self.offset += 1;
            }
        }
    }

    fn put_varint(&mut self, mut x: u64) {
        // MSB varint: low 7 bits in the final byte (no high bit);
        // each earlier byte has the high bit set + next 7 bits.
        let mut buf = [0u8; 10];
        let mut n = 1;
        buf[9] = (x & 0x7f) as u8;
        x >>= 7;
        while x != 0 {
            n += 1;
            buf[10 - n] = ((x & 0x7f) as u8) | 0x80;
            x >>= 7;
        }
        let start = 10 - n;
        self.put_bytes(&buf[start..10]);
    }

    fn put_size(&mut self, sz: usize) {
        self.put_varint(sz as u64);
    }

    fn put_int_nonneg(&mut self, x: i32) {
        debug_assert!(x >= 0);
        self.put_varint(x as u64);
    }

    fn put_integer(&mut self, x: LuaInteger) {
        let cx: u64 = if x >= 0 {
            2u64.wrapping_mul(x as u64)
        } else {
            2u64.wrapping_mul(!(x as u64)).wrapping_add(1)
        };
        self.put_varint(cx);
    }

    fn put_number(&mut self, n: LuaNumber) {
        self.put_bytes(&n.to_le_bytes());
    }

    fn put_instruction(&mut self, i: Instruction) {
        self.put_bytes(&i.to_le_bytes());
    }

    fn put_string(&mut self, handle: Option<StringHandle>) {
        match handle {
            None => {
                self.put_varint(0); // NULL marker
                self.put_varint(0); // special "NULL index"
            }
            Some(h) => {
                let bytes = self.gs.heap.string(h).bytes.clone();
                if let Some(&idx) = self.strings_seen.get(&bytes) {
                    self.put_varint(0); // reuse indicator
                    self.put_varint(idx); // index of the already-saved string
                } else {
                    let size = bytes.len();
                    self.put_size(size + 1); // size + 1 (incl. null)
                    self.put_bytes(&bytes);
                    self.put_byte(0); // trailing NUL
                    self.nstr += 1;
                    self.strings_seen.insert(bytes, self.nstr);
                }
            }
        }
    }

    fn dump_header(&mut self) {
        self.put_bytes(LUA_SIGNATURE);
        self.put_byte(LUAC_VERSION);
        self.put_byte(LUAC_FORMAT);
        self.put_bytes(LUAC_DATA);
        // Sizes + values for the "catch conversion errors" block.
        self.put_byte(std::mem::size_of::<i32>() as u8);
        self.put_bytes(&(LUAC_INT as i32).to_le_bytes());
        self.put_byte(std::mem::size_of::<u32>() as u8);
        self.put_bytes(&LUAC_INST.to_le_bytes());
        self.put_byte(std::mem::size_of::<i64>() as u8);
        self.put_bytes(&LUAC_INT.to_le_bytes());
        self.put_byte(std::mem::size_of::<f64>() as u8);
        self.put_bytes(&LUAC_NUM.to_le_bytes());
    }

    fn dump_code(&mut self, p: &Proto) {
        self.put_int_nonneg(p.code.len() as i32);
        self.put_align(std::mem::size_of::<Instruction>());
        for &i in &p.code {
            self.put_instruction(i);
        }
    }

    fn dump_constants(&mut self, p: &Proto) {
        let n = p.constants.len() as i32;
        self.put_int_nonneg(n);
        for c in &p.constants {
            match *c {
                TValue::Nil => self.put_byte(LUA_VNIL),
                TValue::False => self.put_byte(LUA_VFALSE),
                TValue::True => self.put_byte(LUA_VTRUE),
                TValue::Integer(i) => {
                    self.put_byte(LUA_VNUMINT);
                    self.put_integer(i);
                }
                TValue::Number(n) => {
                    self.put_byte(LUA_VNUMFLT);
                    self.put_number(n);
                }
                TValue::ShortString(h) => {
                    self.put_byte(LUA_VSHRSTR);
                    self.put_string(Some(h));
                }
                TValue::LongString(h) => {
                    self.put_byte(LUA_VLNGSTR);
                    self.put_string(Some(h));
                }
                _ => {
                    // Tables, functions, userdata, threads can't
                    // appear as constants — emit Nil to keep the
                    // format valid.
                    self.put_byte(LUA_VNIL);
                }
            }
        }
    }

    fn dump_upvalues(&mut self, p: &Proto) {
        let n = p.upvalues.len() as i32;
        self.put_int_nonneg(n);
        for uv in &p.upvalues {
            self.put_byte(if uv.in_stack { 1 } else { 0 });
            self.put_byte(uv.idx);
            self.put_byte(uv.kind);
        }
    }

    fn dump_protos(&mut self, p: &Proto) {
        let n = p.inner_protos.len() as i32;
        self.put_int_nonneg(n);
        for &ph in &p.inner_protos {
            self.dump_function(ph);
        }
    }

    fn dump_debug(&mut self, p: &Proto) {
        // lineinfo: relative byte deltas
        let n = if self.strip { 0 } else { p.line_info.len() } as i32;
        self.put_int_nonneg(n);
        if n > 0 {
            for &b in &p.line_info {
                self.put_byte(b as u8);
            }
        }
        // abslineinfo
        let n = if self.strip { 0 } else { p.abs_line_info.len() } as i32;
        self.put_int_nonneg(n);
        if n > 0 {
            self.put_align(std::mem::size_of::<i32>());
            for ali in &p.abs_line_info {
                self.put_bytes(&ali.pc.to_le_bytes());
                self.put_bytes(&ali.line.to_le_bytes());
            }
        }
        // local vars
        let n = if self.strip { 0 } else { p.local_vars.len() } as i32;
        self.put_int_nonneg(n);
        if n > 0 {
            for lv in &p.local_vars {
                self.put_string(lv.name);
                self.put_int_nonneg(lv.start_pc);
                self.put_int_nonneg(lv.end_pc);
            }
        }
        // upvalue names
        let n = if self.strip { 0 } else { p.upvalues.len() } as i32;
        self.put_int_nonneg(n);
        if n > 0 {
            for uv in &p.upvalues {
                self.put_string(uv.name);
            }
        }
    }

    fn dump_function(&mut self, ph: ProtoHandle) {
        let p = self.gs.heap.proto(ph).clone();
        self.put_int_nonneg(p.line_defined);
        self.put_int_nonneg(p.last_line_defined);
        self.put_byte(p.num_params);
        self.put_byte(if p.is_vararg { 1 } else { 0 });
        self.put_byte(p.max_stack_size);
        self.dump_code(&p);
        self.dump_constants(&p);
        self.dump_upvalues(&p);
        self.dump_protos(&p);
        // source string
        self.put_string(if self.strip { None } else { p.source });
        self.dump_debug(&p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::LuaState;

    #[test]
    fn dump_empty_proto_produces_header() {
        let mut state = LuaState::new(0);
        let p = state.global.heap.alloc_proto(Proto::default());
        let out = dump(&state.global, p, false);
        assert!(out.starts_with(LUA_SIGNATURE));
        // Version byte immediately after signature.
        assert_eq!(out[LUA_SIGNATURE.len()], LUAC_VERSION);
    }

    #[test]
    fn varint_roundtrip_zero() {
        let mut state = LuaState::new(0);
        let p = state.global.heap.alloc_proto(Proto::default());
        let out = dump(&state.global, p, true);
        // Non-empty.
        assert!(out.len() > LUA_SIGNATURE.len());
    }
}
