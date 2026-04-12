//! lundump — binary .luac reader.
//!
//! Port of `lundump.c`. Loads a serialised Proto produced by
//! either our own [`crate::ldump::dump`] or C Lua's `luaU_dump`.

#![allow(dead_code)]

use crate::contract::{
    AbsLineInfo, GlobalState, Instruction, LClosure, LClosureHandle, LocVar,
    LuaInteger, Proto, ProtoHandle, StringHandle, TValue, UpValState, UpvalDesc,
};
use crate::ldump::{
    LUAC_DATA, LUAC_FORMAT, LUAC_INST, LUAC_INT, LUAC_NUM, LUAC_VERSION,
    LUA_SIGNATURE, LUA_VFALSE, LUA_VLNGSTR, LUA_VNIL, LUA_VNUMFLT, LUA_VNUMINT,
    LUA_VSHRSTR, LUA_VTRUE,
};

/// Parse a `.luac` byte buffer and return an LClosure handle
/// that wraps the main Proto.
pub fn undump(
    gs: &mut GlobalState,
    bytes: &[u8],
) -> Result<LClosureHandle, String> {
    let mut s = LoadState {
        gs,
        buf: bytes,
        pos: 0,
        strings_seen: Vec::new(),
    };
    s.check_header()?;
    let n_upvals = s.load_byte()?;
    let main_proto = s.load_function()?;
    // Main chunk's upvalue count (from the file header byte) must
    // match proto's upvalue count.
    let expected = s.gs.heap.proto(main_proto).upvalues.len();
    if expected != n_upvals as usize {
        return Err(format!(
            "corrupted chunk: nupvalues mismatch ({} vs {})",
            n_upvals, expected
        ));
    }
    // Create the closure with default Closed(Nil) upvalues; the
    // caller is expected to bind _ENV via parse_with_env-style
    // injection if the closure will be executed directly.
    let mut upvalues = Vec::with_capacity(n_upvals as usize);
    for _ in 0..n_upvals {
        let uv = s.gs.heap.alloc_upval(crate::contract::UpVal {
            state: UpValState::Closed(TValue::Nil),
        });
        upvalues.push(uv);
    }
    let cl = s.gs.heap.alloc_lclosure(LClosure {
        proto: main_proto,
        upvalues,
    });
    Ok(cl)
}

struct LoadState<'a> {
    gs: &'a mut GlobalState,
    buf: &'a [u8],
    pos: usize,
    strings_seen: Vec<StringHandle>,
}

impl<'a> LoadState<'a> {
    fn err<T>(&self, why: &str) -> Result<T, String> {
        Err(format!("bad binary format: {}", why))
    }

    fn load_bytes(&mut self, size: usize) -> Result<&[u8], String> {
        if self.pos + size > self.buf.len() {
            return self.err("truncated chunk");
        }
        let s = &self.buf[self.pos..self.pos + size];
        self.pos += size;
        Ok(s)
    }

    fn load_byte(&mut self) -> Result<u8, String> {
        if self.pos >= self.buf.len() {
            return self.err("truncated chunk");
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn load_align(&mut self, align: usize) -> Result<(), String> {
        let pad = align - (self.pos % align);
        if pad < align {
            for _ in 0..pad {
                let _ = self.load_byte()?;
            }
        }
        Ok(())
    }

    fn load_varint(&mut self) -> Result<u64, String> {
        let mut x: u64 = 0;
        loop {
            let b = self.load_byte()?;
            x = (x << 7) | ((b & 0x7f) as u64);
            if b & 0x80 == 0 {
                return Ok(x);
            }
        }
    }

    fn load_size(&mut self) -> Result<usize, String> {
        Ok(self.load_varint()? as usize)
    }

    fn load_int(&mut self) -> Result<i32, String> {
        Ok(self.load_varint()? as i32)
    }

    fn load_integer(&mut self) -> Result<LuaInteger, String> {
        let cx = self.load_varint()?;
        if cx & 1 != 0 {
            Ok(!(cx >> 1) as i64)
        } else {
            Ok((cx >> 1) as i64)
        }
    }

    fn load_number(&mut self) -> Result<f64, String> {
        let bytes = self.load_bytes(8)?;
        let mut arr = [0u8; 8];
        arr.copy_from_slice(bytes);
        Ok(f64::from_le_bytes(arr))
    }

    fn load_instruction(&mut self) -> Result<Instruction, String> {
        let bytes = self.load_bytes(4)?;
        let mut arr = [0u8; 4];
        arr.copy_from_slice(bytes);
        Ok(u32::from_le_bytes(arr))
    }

    fn load_string(&mut self) -> Result<Option<StringHandle>, String> {
        let size = self.load_size()?;
        if size == 0 {
            // "Reuse" marker. Next varint is the index of the
            // previously-saved string (1-based). idx==0 → NULL.
            let idx = self.load_varint()? as usize;
            if idx == 0 {
                Ok(None)
            } else if idx > self.strings_seen.len() {
                self.err("invalid string index")
            } else {
                Ok(Some(self.strings_seen[idx - 1]))
            }
        } else {
            // Data follows: `size-1` bytes + a trailing NUL.
            let actual = size - 1;
            let bytes = self.load_bytes(actual)?.to_vec();
            let _nul = self.load_byte()?; // consume trailing NUL
            let seed = self.gs.hash_seed;
            let h = self.gs.new_string(&bytes, seed);
            self.strings_seen.push(h);
            Ok(Some(h))
        }
    }

    fn check_header(&mut self) -> Result<(), String> {
        let sig = self.load_bytes(LUA_SIGNATURE.len())?;
        if sig != LUA_SIGNATURE {
            return self.err("not a binary chunk");
        }
        if self.load_byte()? != LUAC_VERSION {
            return self.err("version mismatch");
        }
        if self.load_byte()? != LUAC_FORMAT {
            return self.err("format mismatch");
        }
        let data = self.load_bytes(LUAC_DATA.len())?;
        if data != LUAC_DATA {
            return self.err("corrupted chunk");
        }
        // Check int size + value.
        if self.load_byte()? as usize != std::mem::size_of::<i32>() {
            return self.err("int size mismatch");
        }
        let v = self.load_int32()?;
        if v != LUAC_INT as i32 {
            return self.err("int format mismatch");
        }
        // Instruction
        if self.load_byte()? as usize != std::mem::size_of::<u32>() {
            return self.err("instruction size mismatch");
        }
        let inst = self.load_u32()?;
        if inst != LUAC_INST {
            return self.err("instruction format mismatch");
        }
        // Lua integer
        if self.load_byte()? as usize != std::mem::size_of::<i64>() {
            return self.err("Lua integer size mismatch");
        }
        let li = self.load_i64()?;
        if li != LUAC_INT {
            return self.err("Lua integer format mismatch");
        }
        // Lua number
        if self.load_byte()? as usize != std::mem::size_of::<f64>() {
            return self.err("Lua number size mismatch");
        }
        let ln = self.load_number()?;
        if (ln - LUAC_NUM).abs() > 1e-9 {
            return self.err("Lua number format mismatch");
        }
        Ok(())
    }

    fn load_int32(&mut self) -> Result<i32, String> {
        let bytes = self.load_bytes(4)?;
        let mut arr = [0u8; 4];
        arr.copy_from_slice(bytes);
        Ok(i32::from_le_bytes(arr))
    }
    fn load_u32(&mut self) -> Result<u32, String> {
        let bytes = self.load_bytes(4)?;
        let mut arr = [0u8; 4];
        arr.copy_from_slice(bytes);
        Ok(u32::from_le_bytes(arr))
    }
    fn load_i64(&mut self) -> Result<i64, String> {
        let bytes = self.load_bytes(8)?;
        let mut arr = [0u8; 8];
        arr.copy_from_slice(bytes);
        Ok(i64::from_le_bytes(arr))
    }

    fn load_code(&mut self, p: &mut Proto) -> Result<(), String> {
        let n = self.load_int()? as usize;
        self.load_align(std::mem::size_of::<Instruction>())?;
        let mut code = Vec::with_capacity(n);
        for _ in 0..n {
            code.push(self.load_instruction()?);
        }
        p.code = code;
        Ok(())
    }

    fn load_constants(&mut self, p: &mut Proto) -> Result<(), String> {
        let n = self.load_int()? as usize;
        let mut ks = Vec::with_capacity(n);
        for _ in 0..n {
            let t = self.load_byte()?;
            let v = match t {
                x if x == LUA_VNIL => TValue::Nil,
                x if x == LUA_VFALSE => TValue::False,
                x if x == LUA_VTRUE => TValue::True,
                x if x == LUA_VNUMFLT => TValue::Number(self.load_number()?),
                x if x == LUA_VNUMINT => TValue::Integer(self.load_integer()?),
                x if x == LUA_VSHRSTR => {
                    let h = self.load_string()?;
                    match h {
                        Some(h) => TValue::ShortString(h),
                        None => return self.err("bad constant string"),
                    }
                }
                x if x == LUA_VLNGSTR => {
                    let h = self.load_string()?;
                    match h {
                        Some(h) => TValue::LongString(h),
                        None => return self.err("bad constant string"),
                    }
                }
                _ => return self.err("invalid constant"),
            };
            ks.push(v);
        }
        p.constants = ks;
        Ok(())
    }

    fn load_upvalues(&mut self, p: &mut Proto) -> Result<(), String> {
        let n = self.load_int()? as usize;
        let mut uvs = Vec::with_capacity(n);
        for _ in 0..n {
            let in_stack = self.load_byte()? != 0;
            let idx = self.load_byte()?;
            let kind = self.load_byte()?;
            uvs.push(UpvalDesc {
                name: None,
                in_stack,
                idx,
                kind,
            });
        }
        p.upvalues = uvs;
        Ok(())
    }

    fn load_protos(&mut self, p: &mut Proto) -> Result<(), String> {
        let n = self.load_int()? as usize;
        let mut ps = Vec::with_capacity(n);
        for _ in 0..n {
            ps.push(self.load_function()?);
        }
        p.inner_protos = ps;
        Ok(())
    }

    fn load_debug(&mut self, p: &mut Proto) -> Result<(), String> {
        // line_info
        let n = self.load_int()? as usize;
        let mut lines = Vec::with_capacity(n);
        for _ in 0..n {
            lines.push(self.load_byte()? as i8);
        }
        p.line_info = lines;
        // abs_line_info
        let n = self.load_int()? as usize;
        if n > 0 {
            self.load_align(std::mem::size_of::<i32>())?;
            let mut abs = Vec::with_capacity(n);
            for _ in 0..n {
                let pc = self.load_int32()?;
                let line = self.load_int32()?;
                abs.push(AbsLineInfo { pc, line });
            }
            p.abs_line_info = abs;
        }
        // local_vars
        let n = self.load_int()? as usize;
        let mut lvs = Vec::with_capacity(n);
        for _ in 0..n {
            let name = self.load_string()?;
            let start_pc = self.load_int()?;
            let end_pc = self.load_int()?;
            lvs.push(LocVar { name, start_pc, end_pc });
        }
        p.local_vars = lvs;
        // upvalue names
        let n = self.load_int()? as usize;
        for i in 0..n {
            let name = self.load_string()?;
            if i < p.upvalues.len() {
                p.upvalues[i].name = name;
            }
        }
        Ok(())
    }

    fn load_function(&mut self) -> Result<ProtoHandle, String> {
        let mut p = Proto::default();
        p.line_defined = self.load_int()?;
        p.last_line_defined = self.load_int()?;
        p.num_params = self.load_byte()?;
        p.is_vararg = self.load_byte()? != 0;
        p.max_stack_size = self.load_byte()?;
        self.load_code(&mut p)?;
        self.load_constants(&mut p)?;
        self.load_upvalues(&mut p)?;
        self.load_protos(&mut p)?;
        let src = self.load_string()?;
        p.source = src;
        self.load_debug(&mut p)?;
        Ok(self.gs.heap.alloc_proto(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::LuaState;
    use crate::ldump::dump;

    #[test]
    fn roundtrip_empty_proto() {
        let mut state = LuaState::new(0);
        let ph = state.global.heap.alloc_proto(Proto {
            max_stack_size: 2,
            is_vararg: true,
            ..Proto::default()
        });
        // The main chunk needs an _ENV upvalue descriptor so
        // nupvals > 0 matches after roundtrip.
        state.global.heap.proto_mut(ph).upvalues.push(UpvalDesc {
            name: None,
            in_stack: true,
            idx: 0,
            kind: 0,
        });
        let bytes = dump(&state.global, ph, true);
        let cl = undump(&mut state.global, &bytes).expect("undump");
        let cl_proto = state.global.heap.lclosure(cl).proto;
        let p = state.global.heap.proto(cl_proto);
        assert_eq!(p.max_stack_size, 2);
        assert!(p.is_vararg);
        assert_eq!(p.upvalues.len(), 1);
    }

    #[test]
    fn roundtrip_compiled_return_expression() {
        let mut state = LuaState::new(0);
        let globals = crate::lbaselib::open_base(&mut state);
        let closure = crate::lparser::parse_with_env(
            &mut state,
            b"return 1 + 2 + 3",
            b"=test",
            globals,
        );
        let proto_handle = state.global.heap.lclosure(closure).proto;
        let bytes = dump(&state.global, proto_handle, true);
        assert!(!bytes.is_empty());
        // Round-trip via undump.
        let cl = undump(&mut state.global, &bytes).expect("undump");
        state.current_thread_mut().push(TValue::LuaClosure(cl));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(6));
    }
}
