//! Tag methods — metamethod event names and lookup helpers.
//!
//! Stage 2 / commit 6. Ports the static parts of `ltm.c`:
//!
//! * [`TagMethod`] — enum mirroring C's `TMS` with the same 25
//!   variants in the same order. The fast-access methods
//!   (`Index..Eq`) come first so the 6-bit `meta_cache_flags` on
//!   [`crate::contract::Table`] can cache their absence.
//! * [`EVENT_NAMES`] — 25 byte-slice constants byte-identical with
//!   the `luaT_eventname` list in `luaT_init`.
//! * [`TYPE_NAMES`] — extended type name table used by
//!   `luaT_objtypename` and `lua_typename`. Byte-identical with
//!   `luaT_typenames_` in `ltm.c`.
//! * [`GlobalState::init_metamethod_names`] — intern every event
//!   name, storing the handles in `GlobalState::tm_names`. Matches
//!   the `luaT_init` loop.
//! * [`GlobalState::tag_method_name`] — look up the interned handle
//!   for a given [`TagMethod`].
//!
//! NOT ported in Stage 2:
//!
//! * `luaT_gettm` / `luaT_gettmbyobj` / `luaT_callTM` /
//!   `luaT_trybinTM` family — the actual metamethod dispatch and
//!   fast-path lookup. These need `ltable::getshortstr` and the VM's
//!   callable-frame machinery, both of which come in Stages 4-5.
//! * `luaT_adjustvarargs` / `luaT_getvararg*` — vararg handling on
//!   function entry, Stage 5.

#![allow(dead_code)]

use crate::contract::{GlobalState, StringHandle};
use crate::lstring::LUAI_MAXSHORTLEN;

/// Tag method identifiers. The ordering of the first seven variants
/// (through [`Eq`]) is load-bearing: the `meta_cache_flags` byte on
/// each [`crate::contract::Table`] uses bits `0..=TM_EQ` to cache the
/// absence of fast-access metamethods. **Do not reorder without
/// updating the `maskflags` macro in `ltm.h` and every OP_MMBIN*
/// opcode's behavior.**
///
/// Matches the `TMS` enum in `ltm.h` byte-for-byte.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagMethod {
    Index = 0,
    NewIndex = 1,
    Gc = 2,
    Mode = 3,
    Len = 4,
    Eq = 5,
    // --- last fast-access method above this line ---
    Add = 6,
    Sub = 7,
    Mul = 8,
    Mod = 9,
    Pow = 10,
    Div = 11,
    IDiv = 12,
    BAnd = 13,
    BOr = 14,
    BXor = 15,
    Shl = 16,
    Shr = 17,
    Unm = 18,
    BNot = 19,
    Lt = 20,
    Le = 21,
    Concat = 22,
    Call = 23,
    Close = 24,
}

/// Total number of tag method events. Matches `TM_N` in `ltm.h`.
pub const TM_N: usize = 25;

/// Last tag method with fast-access cache bit (`meta_cache_flags` on
/// [`crate::contract::Table`]). Matches `TM_EQ` in `ltm.h`.
pub const TM_LAST_FAST: TagMethod = TagMethod::Eq;

impl TagMethod {
    /// Convert the numeric tag byte back to a [`TagMethod`]. Returns
    /// `None` for any value outside `0..TM_N`.
    pub const fn from_raw(raw: u8) -> Option<TagMethod> {
        match raw {
            0 => Some(TagMethod::Index),
            1 => Some(TagMethod::NewIndex),
            2 => Some(TagMethod::Gc),
            3 => Some(TagMethod::Mode),
            4 => Some(TagMethod::Len),
            5 => Some(TagMethod::Eq),
            6 => Some(TagMethod::Add),
            7 => Some(TagMethod::Sub),
            8 => Some(TagMethod::Mul),
            9 => Some(TagMethod::Mod),
            10 => Some(TagMethod::Pow),
            11 => Some(TagMethod::Div),
            12 => Some(TagMethod::IDiv),
            13 => Some(TagMethod::BAnd),
            14 => Some(TagMethod::BOr),
            15 => Some(TagMethod::BXor),
            16 => Some(TagMethod::Shl),
            17 => Some(TagMethod::Shr),
            18 => Some(TagMethod::Unm),
            19 => Some(TagMethod::BNot),
            20 => Some(TagMethod::Lt),
            21 => Some(TagMethod::Le),
            22 => Some(TagMethod::Concat),
            23 => Some(TagMethod::Call),
            24 => Some(TagMethod::Close),
            _ => None,
        }
    }

    /// Bit mask for the `meta_cache_flags` cache bit that marks this
    /// tag method as "absent" on a metatable. Only valid for the fast-
    /// access methods (`Index..=Eq`); panics otherwise.
    pub const fn cache_bit(self) -> u8 {
        assert!(
            (self as u8) <= TagMethod::Eq as u8,
            "cache_bit is only defined for fast-access methods (Index..=Eq)"
        );
        1u8 << (self as u8)
    }
}

/// Metamethod event names, byte-identical with the `luaT_eventname`
/// array in `luaT_init` in `ltm.c`. Indexed by `TagMethod as usize`.
#[rustfmt::skip]
pub const EVENT_NAMES: [&[u8]; TM_N] = [
    b"__index",
    b"__newindex",
    b"__gc",
    b"__mode",
    b"__len",
    b"__eq",
    b"__add",
    b"__sub",
    b"__mul",
    b"__mod",
    b"__pow",
    b"__div",
    b"__idiv",
    b"__band",
    b"__bor",
    b"__bxor",
    b"__shl",
    b"__shr",
    b"__unm",
    b"__bnot",
    b"__lt",
    b"__le",
    b"__concat",
    b"__call",
    b"__close",
];

/// Extended type-name table used by `luaT_objtypename` and
/// `lua_typename`. Indexed by the extended tag space
/// `0..=LUA_TOTALTYPES`. Matches `luaT_typenames_` in `ltm.c`.
///
/// Index mapping (from `lobject.h`):
///
/// | index | tag               | name       |
/// |-------|-------------------|------------|
/// | 0     | no value          | "no value" |
/// | 1     | LUA_TNIL          | "nil"      |
/// | 2     | LUA_TBOOLEAN      | "boolean"  |
/// | 3     | LUA_TLIGHTUSERDATA| "userdata" |
/// | 4     | LUA_TNUMBER       | "number"   |
/// | 5     | LUA_TSTRING       | "string"   |
/// | 6     | LUA_TTABLE        | "table"    |
/// | 7     | LUA_TFUNCTION     | "function" |
/// | 8     | LUA_TUSERDATA     | "userdata" |
/// | 9     | LUA_TTHREAD       | "thread"   |
/// | 10    | LUA_TUPVAL        | "upvalue"  |
/// | 11    | LUA_TPROTO        | "proto"    |
#[rustfmt::skip]
pub const TYPE_NAMES: [&[u8]; 12] = [
    b"no value",
    b"nil",
    b"boolean",
    b"userdata",
    b"number",
    b"string",
    b"table",
    b"function",
    b"userdata",
    b"thread",
    b"upvalue",
    b"proto",
];

impl GlobalState {
    /// Intern every metamethod event name from [`EVENT_NAMES`] and
    /// store the resulting handles in `self.tm_names`. Idempotent — a
    /// second call reuses the already-interned handles but re-fills
    /// `tm_names` (which is a no-op since interning is deterministic).
    ///
    /// Matches the `luaT_init` loop in `ltm.c`. The C version also
    /// calls `luaC_fix` on each name so the GC never collects them;
    /// our Stage 2 port does not have a GC yet (Stage 3), so the
    /// "pinned" semantics happen automatically — nothing ever frees
    /// these handles.
    pub fn init_metamethod_names(&mut self) {
        let mut names = Vec::with_capacity(TM_N);
        let seed = self.hash_seed;
        for &event_bytes in EVENT_NAMES.iter() {
            // All event names fit inside LUAI_MAXSHORTLEN, so we can
            // go through the short-string intern path directly.
            debug_assert!(event_bytes.len() <= LUAI_MAXSHORTLEN);
            let handle = self.intern_short(event_bytes, seed);
            names.push(handle);
        }
        self.tm_names = names;
    }

    /// Return the interned [`StringHandle`] for a given tag method.
    ///
    /// # Panics
    ///
    /// Panics if [`init_metamethod_names`] has not been called yet.
    pub fn tag_method_name(&self, event: TagMethod) -> StringHandle {
        assert!(
            !self.tm_names.is_empty(),
            "tag_method_name called before init_metamethod_names"
        );
        self.tm_names[event as usize]
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tm_n_is_25() {
        assert_eq!(TM_N, 25);
        assert_eq!(EVENT_NAMES.len(), TM_N);
    }

    #[test]
    fn every_event_name_starts_with_double_underscore() {
        for name in EVENT_NAMES {
            assert!(name.starts_with(b"__"));
        }
    }

    #[test]
    fn every_event_name_fits_in_short_string() {
        for name in EVENT_NAMES {
            assert!(name.len() <= LUAI_MAXSHORTLEN);
        }
    }

    #[test]
    fn event_names_match_tagmethod_order() {
        assert_eq!(EVENT_NAMES[TagMethod::Index as usize], b"__index");
        assert_eq!(EVENT_NAMES[TagMethod::NewIndex as usize], b"__newindex");
        assert_eq!(EVENT_NAMES[TagMethod::Add as usize], b"__add");
        assert_eq!(EVENT_NAMES[TagMethod::Close as usize], b"__close");
    }

    #[test]
    fn tag_method_from_raw_roundtrips() {
        for i in 0u8..(TM_N as u8) {
            let tm = TagMethod::from_raw(i).expect("valid tag method");
            assert_eq!(tm as u8, i);
        }
        assert!(TagMethod::from_raw(TM_N as u8).is_none());
        assert!(TagMethod::from_raw(255).is_none());
    }

    #[test]
    fn cache_bits_cover_fast_access_methods() {
        assert_eq!(TagMethod::Index.cache_bit(), 0b0000_0001);
        assert_eq!(TagMethod::NewIndex.cache_bit(), 0b0000_0010);
        assert_eq!(TagMethod::Gc.cache_bit(), 0b0000_0100);
        assert_eq!(TagMethod::Mode.cache_bit(), 0b0000_1000);
        assert_eq!(TagMethod::Len.cache_bit(), 0b0001_0000);
        assert_eq!(TagMethod::Eq.cache_bit(), 0b0010_0000);
    }

    #[test]
    fn init_metamethod_names_populates_all_25() {
        let mut g = GlobalState::default();
        g.init_metamethod_names();
        assert_eq!(g.tm_names.len(), TM_N);
        // Every name reads back to the expected bytes.
        for (i, &handle) in g.tm_names.clone().iter().enumerate() {
            assert_eq!(g.heap.string(handle).bytes, EVENT_NAMES[i]);
        }
    }

    #[test]
    fn init_metamethod_names_is_idempotent() {
        let mut g = GlobalState::default();
        g.init_metamethod_names();
        let first_handles = g.tm_names.clone();
        g.init_metamethod_names();
        // Intern returns the same handle for the same content, so the
        // second call produces the same list (not necessarily the
        // same Vec identity, but the same contents).
        assert_eq!(g.tm_names, first_handles);
    }

    #[test]
    fn tag_method_name_accesses_interned_handle() {
        let mut g = GlobalState::default();
        g.init_metamethod_names();
        let h = g.tag_method_name(TagMethod::Add);
        assert_eq!(g.heap.string(h).bytes, b"__add");
    }

    #[test]
    #[should_panic(expected = "tag_method_name called before init_metamethod_names")]
    fn tag_method_name_before_init_panics() {
        let g = GlobalState::default();
        let _ = g.tag_method_name(TagMethod::Index);
    }

    #[test]
    fn type_names_have_twelve_entries() {
        assert_eq!(TYPE_NAMES.len(), 12);
        assert_eq!(TYPE_NAMES[0], b"no value");
        assert_eq!(TYPE_NAMES[1], b"nil");
        assert_eq!(TYPE_NAMES[7], b"function");
        assert_eq!(TYPE_NAMES[11], b"proto");
    }
}
