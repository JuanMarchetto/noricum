//! linit — initialisation of all standard libraries.
//!
//! Port of `linit.c`. `open_libs(state)` registers every standard
//! library onto a fresh globals table and returns the handle so
//! callers can bind it as `_ENV` on compiled closures.

#![allow(dead_code)]

use crate::contract::{LuaState, TableHandle};

/// Open all standard libraries. Returns the globals table that
/// holds every registered library (and should be bound as `_ENV`
/// on compiled scripts).
pub fn open_libs(state: &mut LuaState) -> TableHandle {
    let globals = crate::lbaselib::open_base(state);
    crate::lstrlib::open_string(state, globals);
    crate::lmathlib::open_math(state, globals);
    crate::ltablib::open_table(state, globals);
    crate::loslib::open_os(state, globals);
    crate::liolib::open_io(state, globals);
    crate::ldblib::open_debug(state, globals);
    crate::lcorolib::open_coroutine(state, globals);
    crate::lutf8lib::open_utf8(state, globals);
    crate::loadlib::open_package(state, globals);
    state.global.globals = Some(globals);
    globals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::TValue;

    #[test]
    fn open_libs_registers_all_library_tables() {
        let mut state = LuaState::new(0);
        let g = open_libs(&mut state);
        for lib in &[
            "string", "math", "table", "os", "io", "debug",
            "coroutine", "utf8",
        ] {
            let name = state.global.new_string(lib.as_bytes(), 0);
            let v = state.global.heap.table_get_shortstr(g, name);
            assert!(
                matches!(v, Some(TValue::Table(_))),
                "missing standard library: {}",
                lib
            );
        }
    }

    #[test]
    fn debug_simple_math_lookup() {
        let mut state = LuaState::new(0);
        let g = open_libs(&mut state);
        let closure = crate::lparser::parse_with_env(
            &mut state,
            b"return math",
            b"=test",
            g,
        );
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert!(matches!(
            state.current_thread().stack[0],
            TValue::Table(_)
        ));
    }

    #[test]
    fn math_floor_lookup_returns_function() {
        let mut state = LuaState::new(0);
        let g = open_libs(&mut state);
        let closure = crate::lparser::parse_with_env(
            &mut state,
            b"return math.floor",
            b"=test",
            g,
        );
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert!(matches!(
            state.current_thread().stack[0],
            TValue::LightCFunction(_)
        ));
    }

    #[test]
    fn debug_tokenize_math_floor() {
        use crate::llex::{LexState, TK_EOS};
        use crate::contract::GlobalState;
        let mut gs = GlobalState::default();
        gs.init_metamethod_names();
        let name = gs.new_string(b"=test", 0);
        let src = b"return math.floor(3.7)";
        let mut ls = LexState::new(&mut gs, src, name);
        let mut toks = Vec::new();
        loop {
            ls.next_token();
            if ls.t.token == TK_EOS { break; }
            toks.push(ls.t.token);
        }
        // Expect: RETURN, NAME(math), '.', NAME(floor), '(', FLT(3.7), ')'
        assert_eq!(toks.len(), 7, "tokens: {:?}", toks);
    }

    #[test]
    fn open_libs_runs_lua_script_using_math() {
        let mut state = LuaState::new(0);
        let g = open_libs(&mut state);
        let closure = crate::lparser::parse_with_env(
            &mut state,
            b"return math.floor(3.7)",
            b"=test",
            g,
        );
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        // Accept either Integer(3) or Number(3.0) since floor may
        // land in either representation depending on the path.
        let got = state.current_thread().stack[0];
        match got {
            TValue::Integer(3) => {}
            TValue::Number(n) if (n - 3.0).abs() < 1e-9 => {}
            other => panic!("expected 3, got {:?}", other),
        }
    }

    #[test]
    fn open_libs_runs_lua_script_using_string() {
        let mut state = LuaState::new(0);
        let g = open_libs(&mut state);
        let closure = crate::lparser::parse_with_env(
            &mut state,
            b"return string.upper(\"hello\")",
            b"=test",
            g,
        );
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_lstring(1), Some(b"HELLO".as_slice()));
    }
}
