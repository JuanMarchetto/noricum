//! Differential test for `ltm::EVENT_NAMES`.
//!
//! The metamethod event names are baked into `.luac` bytecode via
//! `lua_getfield(L, -1, "__index")`-style calls the compiler emits
//! against strings read from these names at state init. If our table
//! drifts from C, every metamethod dispatch in a chunk compiled by
//! our port will miss silently. The oracle shim spins up a real Lua
//! state (which runs `luaT_init` internally) and hands back the nth
//! interned name via `G(L)->tmname[i]`; we compare byte-for-byte.

use spike::ltm::{EVENT_NAMES, TM_N};
use spike::{wr_ltm_event_name, wr_ltm_tm_n};

fn oracle_name(i: i32) -> Vec<u8> {
    let mut buf = [0u8; 32];
    let len = unsafe { wr_ltm_event_name(i, buf.as_mut_ptr(), buf.len()) };
    assert!(
        len > 0,
        "oracle failed to fetch event name {i} (len = {len})"
    );
    buf[..len as usize].to_vec()
}

#[test]
fn tm_n_matches_c() {
    let c = unsafe { wr_ltm_tm_n() };
    assert_eq!(TM_N as i32, c);
}

#[test]
fn every_event_name_matches_c_oracle() {
    for (i, &rust) in EVENT_NAMES.iter().enumerate() {
        let c = oracle_name(i as i32);
        assert_eq!(
            rust,
            c.as_slice(),
            "event name {i}: rust={:?}, c={:?}",
            std::str::from_utf8(rust).unwrap_or("?"),
            std::str::from_utf8(&c).unwrap_or("?")
        );
    }
}
