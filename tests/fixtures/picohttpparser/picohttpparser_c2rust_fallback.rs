#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unused_assignments,
    unused_mut
)]
#![feature(c_variadic, label_break_value, raw_ref_op)]
extern "C" {
    fn __assert_fail(
        __assertion: *const ::core::ffi::c_char,
        __file: *const ::core::ffi::c_char,
        __line: ::core::ffi::c_uint,
        __function: *const ::core::ffi::c_char,
    ) -> !;
    fn printf(__format: *const ::core::ffi::c_char, ...) -> ::core::ffi::c_int;
    fn sprintf(
        __s: *mut ::core::ffi::c_char,
        __format: *const ::core::ffi::c_char,
        ...
    ) -> ::core::ffi::c_int;
    fn vprintf(
        __format: *const ::core::ffi::c_char,
        __arg: ::core::ffi::VaList,
    ) -> ::core::ffi::c_int;
    fn malloc(__size: size_t) -> *mut ::core::ffi::c_void;
    fn free(__ptr: *mut ::core::ffi::c_void);
    fn memcpy(
        __dest: *mut ::core::ffi::c_void,
        __src: *const ::core::ffi::c_void,
        __n: size_t,
    ) -> *mut ::core::ffi::c_void;
    fn memmove(
        __dest: *mut ::core::ffi::c_void,
        __src: *const ::core::ffi::c_void,
        __n: size_t,
    ) -> *mut ::core::ffi::c_void;
    fn memset(
        __s: *mut ::core::ffi::c_void,
        __c: ::core::ffi::c_int,
        __n: size_t,
    ) -> *mut ::core::ffi::c_void;
    fn memcmp(
        __s1: *const ::core::ffi::c_void,
        __s2: *const ::core::ffi::c_void,
        __n: size_t,
    ) -> ::core::ffi::c_int;
    fn strcpy(
        __dest: *mut ::core::ffi::c_char,
        __src: *const ::core::ffi::c_char,
    ) -> *mut ::core::ffi::c_char;
    fn strdup(__s: *const ::core::ffi::c_char) -> *mut ::core::ffi::c_char;
    fn strlen(__s: *const ::core::ffi::c_char) -> size_t;
}
pub type __builtin_va_list = [__va_list_tag; 1];
#[derive(Copy, Clone)]
#[repr(C)]
pub struct __va_list_tag {
    pub gp_offset: ::core::ffi::c_uint,
    pub fp_offset: ::core::ffi::c_uint,
    pub overflow_arg_area: *mut ::core::ffi::c_void,
    pub reg_save_area: *mut ::core::ffi::c_void,
}
pub type va_list = __builtin_va_list;
pub type size_t = usize;
pub type __uint64_t = u64;
pub type __ssize_t = ::core::ffi::c_long;
pub type uint64_t = __uint64_t;
pub type ssize_t = __ssize_t;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct test_t {
    pub num_tests: ::core::ffi::c_int,
    pub failed: ::core::ffi::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct phr_header {
    pub name: *const ::core::ffi::c_char,
    pub name_len: size_t,
    pub value: *const ::core::ffi::c_char,
    pub value_len: size_t,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct phr_chunked_decoder {
    pub bytes_left_in_chunk: size_t,
    pub consume_trailer: ::core::ffi::c_char,
    pub _hex_count: ::core::ffi::c_char,
    pub _state: ::core::ffi::c_char,
    pub _total_read: uint64_t,
    pub _total_overhead: uint64_t,
}
pub type C2RustUnnamed = ::core::ffi::c_uint;
pub const CHUNKED_IN_TRAILERS_LINE_MIDDLE: C2RustUnnamed = 7;
pub const CHUNKED_IN_TRAILERS_LINE_HEAD: C2RustUnnamed = 6;
pub const CHUNKED_IN_CHUNK_DATA_EXPECT_LF: C2RustUnnamed = 5;
pub const CHUNKED_IN_CHUNK_DATA_EXPECT_CR: C2RustUnnamed = 4;
pub const CHUNKED_IN_CHUNK_DATA: C2RustUnnamed = 3;
pub const CHUNKED_IN_CHUNK_HEADER_EXPECT_LF: C2RustUnnamed = 2;
pub const CHUNKED_IN_CHUNK_EXT: C2RustUnnamed = 1;
pub const CHUNKED_IN_CHUNK_SIZE: C2RustUnnamed = 0;
pub const NULL: *mut ::core::ffi::c_void = ::core::ptr::null_mut::<::core::ffi::c_void>();
static mut cur_tests: *mut test_t = unsafe { &raw const main_tests as *mut test_t };
static mut main_tests: test_t = test_t {
    num_tests: 0,
    failed: 0,
};
static mut test_level: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
unsafe extern "C" fn indent() {
    let mut i: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    while i != test_level {
        printf(b"    \0" as *const u8 as *const ::core::ffi::c_char);
        i += 1;
    }
}
unsafe extern "C" fn note(mut fmt: *const ::core::ffi::c_char, mut args: ...) {
    let mut arg: ::core::ffi::VaListImpl;
    indent();
    printf(b"# \0" as *const u8 as *const ::core::ffi::c_char);
    arg = args.clone();
    vprintf(fmt, arg.as_va_list());
    printf(b"\n\0" as *const u8 as *const ::core::ffi::c_char);
}
unsafe extern "C" fn _ok(
    mut cond: ::core::ffi::c_int,
    mut fmt: *const ::core::ffi::c_char,
    mut args: ...
) {
    let mut arg: ::core::ffi::VaListImpl;
    if cond == 0 {
        (*cur_tests).failed = 1 as ::core::ffi::c_int;
    }
    indent();
    (*cur_tests).num_tests += 1;
    printf(
        b"%s %d - \0" as *const u8 as *const ::core::ffi::c_char,
        if cond != 0 {
            b"ok\0" as *const u8 as *const ::core::ffi::c_char
        } else {
            b"not ok\0" as *const u8 as *const ::core::ffi::c_char
        },
        (*cur_tests).num_tests,
    );
    arg = args.clone();
    vprintf(fmt, arg.as_va_list());
    printf(b"\n\0" as *const u8 as *const ::core::ffi::c_char);
}
unsafe extern "C" fn done_testing() -> ::core::ffi::c_int {
    indent();
    printf(
        b"1..%d\n\0" as *const u8 as *const ::core::ffi::c_char,
        (*cur_tests).num_tests,
    );
    return (*cur_tests).failed;
}
unsafe extern "C" fn subtest(
    mut name: *const ::core::ffi::c_char,
    mut cb: Option<unsafe extern "C" fn() -> ()>,
) {
    let mut test: test_t = test_t {
        num_tests: 0 as ::core::ffi::c_int,
        failed: 0,
    };
    let mut parent_tests: *mut test_t = ::core::ptr::null_mut::<test_t>();
    parent_tests = cur_tests;
    cur_tests = &raw mut test;
    test_level += 1;
    note(
        b"Subtest: %s\0" as *const u8 as *const ::core::ffi::c_char,
        name,
    );
    cb.expect("non-null function pointer")();
    done_testing();
    test_level -= 1;
    cur_tests = parent_tests;
    if test.failed != 0 {
        (*cur_tests).failed = 1 as ::core::ffi::c_int;
    }
    _ok(
        (test.failed == 0) as ::core::ffi::c_int,
        b"%s\0" as *const u8 as *const ::core::ffi::c_char,
        name,
    );
}
static mut token_char_map: *const ::core::ffi::c_char = b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01\0\x01\x01\x01\x01\x01\0\0\x01\x01\0\x01\x01\0\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\0\0\0\0\0\0\0\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\0\0\0\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\x01\0\x01\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
    as *const u8 as *const ::core::ffi::c_char;
unsafe extern "C" fn findchar_fast(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut ranges: *const ::core::ffi::c_char,
    mut ranges_size: size_t,
    mut found: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    *found = 0 as ::core::ffi::c_int;
    return buf;
}
unsafe extern "C" fn get_token_to_eol(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut token: *mut *const ::core::ffi::c_char,
    mut token_len: *mut size_t,
    mut ret: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    let mut current_block: u64;
    let mut token_start: *const ::core::ffi::c_char = buf;
    loop {
        if !((buf_end.offset_from(buf) as ::core::ffi::c_long >= 8 as ::core::ffi::c_long)
            as ::core::ffi::c_int as ::core::ffi::c_long
            != 0)
        {
            current_block = 17281240262373992796;
            break;
        }
        if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
            .wrapping_sub(0o40 as ::core::ffi::c_uint)
            < 0o137 as ::core::ffi::c_uint) as ::core::ffi::c_int
            as ::core::ffi::c_long
            != 0)
        {
            buf = buf.offset(1);
            if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                .wrapping_sub(0o40 as ::core::ffi::c_uint)
                < 0o137 as ::core::ffi::c_uint) as ::core::ffi::c_int
                as ::core::ffi::c_long
                != 0)
            {
                buf = buf.offset(1);
                if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                    .wrapping_sub(0o40 as ::core::ffi::c_uint)
                    < 0o137 as ::core::ffi::c_uint) as ::core::ffi::c_int
                    as ::core::ffi::c_long
                    != 0)
                {
                    buf = buf.offset(1);
                    if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                        .wrapping_sub(0o40 as ::core::ffi::c_uint)
                        < 0o137 as ::core::ffi::c_uint)
                        as ::core::ffi::c_int as ::core::ffi::c_long
                        != 0)
                    {
                        buf = buf.offset(1);
                        if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                            .wrapping_sub(0o40 as ::core::ffi::c_uint)
                            < 0o137 as ::core::ffi::c_uint)
                            as ::core::ffi::c_int
                            as ::core::ffi::c_long
                            != 0)
                        {
                            buf = buf.offset(1);
                            if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                                .wrapping_sub(0o40 as ::core::ffi::c_uint)
                                < 0o137 as ::core::ffi::c_uint)
                                as ::core::ffi::c_int
                                as ::core::ffi::c_long
                                != 0)
                            {
                                buf = buf.offset(1);
                                if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                                    .wrapping_sub(0o40 as ::core::ffi::c_uint)
                                    < 0o137 as ::core::ffi::c_uint)
                                    as ::core::ffi::c_int
                                    as ::core::ffi::c_long
                                    != 0)
                                {
                                    buf = buf.offset(1);
                                    if !(!((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                                        .wrapping_sub(0o40 as ::core::ffi::c_uint)
                                        < 0o137 as ::core::ffi::c_uint)
                                        as ::core::ffi::c_int
                                        as ::core::ffi::c_long
                                        != 0)
                                    {
                                        buf = buf.offset(1);
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if ((*buf as ::core::ffi::c_uchar as ::core::ffi::c_int) < ' ' as i32) as ::core::ffi::c_int
            as ::core::ffi::c_long
            != 0
            && (*buf as ::core::ffi::c_int != '\t' as i32) as ::core::ffi::c_int
                as ::core::ffi::c_long
                != 0
            || (*buf as ::core::ffi::c_int == '\u{7f}' as i32) as ::core::ffi::c_int
                as ::core::ffi::c_long
                != 0
        {
            current_block = 1970500584204316837;
            break;
        }
        buf = buf.offset(1);
    }
    loop {
        match current_block {
            1970500584204316837 => {
                if (*buf as ::core::ffi::c_int == '\r' as i32) as ::core::ffi::c_int
                    as ::core::ffi::c_long
                    != 0
                {
                    buf = buf.offset(1);
                    if buf == buf_end {
                        *ret = -(2 as ::core::ffi::c_int);
                        return ::core::ptr::null::<::core::ffi::c_char>();
                    }
                    let fresh0 = buf;
                    buf = buf.offset(1);
                    if *fresh0 as ::core::ffi::c_int != '\n' as i32 {
                        *ret = -(1 as ::core::ffi::c_int);
                        return ::core::ptr::null::<::core::ffi::c_char>();
                    }
                    *token_len = buf
                        .offset(-(2 as ::core::ffi::c_int as isize))
                        .offset_from(token_start)
                        as ::core::ffi::c_long as size_t;
                } else if *buf as ::core::ffi::c_int == '\n' as i32 {
                    *token_len = buf.offset_from(token_start) as ::core::ffi::c_long as size_t;
                    buf = buf.offset(1);
                } else {
                    *ret = -(1 as ::core::ffi::c_int);
                    return ::core::ptr::null::<::core::ffi::c_char>();
                }
                break;
            }
            _ => {
                if buf == buf_end {
                    *ret = -(2 as ::core::ffi::c_int);
                    return ::core::ptr::null::<::core::ffi::c_char>();
                }
                if !((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
                    .wrapping_sub(0o40 as ::core::ffi::c_uint)
                    < 0o137 as ::core::ffi::c_uint) as ::core::ffi::c_int
                    as ::core::ffi::c_long
                    != 0
                {
                    if ((*buf as ::core::ffi::c_uchar as ::core::ffi::c_int) < ' ' as i32)
                        as ::core::ffi::c_int as ::core::ffi::c_long
                        != 0
                        && (*buf as ::core::ffi::c_int != '\t' as i32) as ::core::ffi::c_int
                            as ::core::ffi::c_long
                            != 0
                        || (*buf as ::core::ffi::c_int == '\u{7f}' as i32) as ::core::ffi::c_int
                            as ::core::ffi::c_long
                            != 0
                    {
                        current_block = 1970500584204316837;
                        continue;
                    }
                }
                buf = buf.offset(1);
                current_block = 17281240262373992796;
            }
        }
    }
    *token = token_start;
    return buf;
}
unsafe extern "C" fn is_complete(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut last_len: size_t,
    mut ret: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    let mut ret_cnt: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    buf = if last_len < 3 as size_t {
        buf
    } else {
        buf.offset(last_len as isize)
            .offset(-(3 as ::core::ffi::c_int as isize))
    };
    loop {
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        if *buf as ::core::ffi::c_int == '\r' as i32 {
            buf = buf.offset(1);
            if buf == buf_end {
                *ret = -(2 as ::core::ffi::c_int);
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
            if buf == buf_end {
                *ret = -(2 as ::core::ffi::c_int);
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
            let fresh1 = buf;
            buf = buf.offset(1);
            if *fresh1 as ::core::ffi::c_int != '\n' as i32 {
                *ret = -(1 as ::core::ffi::c_int);
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
            ret_cnt += 1;
        } else if *buf as ::core::ffi::c_int == '\n' as i32 {
            buf = buf.offset(1);
            ret_cnt += 1;
        } else {
            buf = buf.offset(1);
            ret_cnt = 0 as ::core::ffi::c_int;
        }
        if ret_cnt == 2 as ::core::ffi::c_int {
            return buf;
        }
    }
}
unsafe extern "C" fn parse_token(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut token: *mut *const ::core::ffi::c_char,
    mut token_len: *mut size_t,
    mut next_char: ::core::ffi::c_char,
    mut ret: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    static mut ranges: [::core::ffi::c_char; 17] = unsafe {
        ::core::mem::transmute::<[u8; 17], [::core::ffi::c_char; 17]>(*b"\0 \"\"(),,//:@[]{\xFF\0")
    };
    let mut buf_start: *const ::core::ffi::c_char = buf;
    let mut found: ::core::ffi::c_int = 0;
    buf = findchar_fast(
        buf,
        buf_end,
        &raw const ranges as *const ::core::ffi::c_char,
        (::core::mem::size_of::<[::core::ffi::c_char; 17]>() as size_t).wrapping_sub(1 as size_t),
        &raw mut found,
    );
    if found == 0 {
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
    }
    while !(*buf as ::core::ffi::c_int == next_char as ::core::ffi::c_int) {
        if *token_char_map.offset(*buf as ::core::ffi::c_uchar as isize) == 0 {
            *ret = -(1 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        buf = buf.offset(1);
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
    }
    *token = buf_start;
    *token_len = buf.offset_from(buf_start) as ::core::ffi::c_long as size_t;
    return buf;
}
unsafe extern "C" fn parse_http_version(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut minor_version: *mut ::core::ffi::c_int,
    mut ret: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    if (buf_end.offset_from(buf) as ::core::ffi::c_long) < 9 as ::core::ffi::c_long {
        *ret = -(2 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh2 = buf;
    buf = buf.offset(1);
    if *fresh2 as ::core::ffi::c_int != 'H' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh3 = buf;
    buf = buf.offset(1);
    if *fresh3 as ::core::ffi::c_int != 'T' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh4 = buf;
    buf = buf.offset(1);
    if *fresh4 as ::core::ffi::c_int != 'T' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh5 = buf;
    buf = buf.offset(1);
    if *fresh5 as ::core::ffi::c_int != 'P' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh6 = buf;
    buf = buf.offset(1);
    if *fresh6 as ::core::ffi::c_int != '/' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh7 = buf;
    buf = buf.offset(1);
    if *fresh7 as ::core::ffi::c_int != '1' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh8 = buf;
    buf = buf.offset(1);
    if *fresh8 as ::core::ffi::c_int != '.' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    if (*buf as ::core::ffi::c_int) < '0' as i32 || ('9' as i32) < *buf as ::core::ffi::c_int {
        buf = buf.offset(1);
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh9 = buf;
    buf = buf.offset(1);
    *minor_version = 1 as ::core::ffi::c_int * (*fresh9 as ::core::ffi::c_int - '0' as i32);
    return buf;
}
unsafe extern "C" fn parse_headers(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut headers: *mut phr_header,
    mut num_headers: *mut size_t,
    mut max_headers: size_t,
    mut ret: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    loop {
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        if *buf as ::core::ffi::c_int == '\r' as i32 {
            buf = buf.offset(1);
            if buf == buf_end {
                *ret = -(2 as ::core::ffi::c_int);
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
            let fresh10 = buf;
            buf = buf.offset(1);
            if *fresh10 as ::core::ffi::c_int != '\n' as i32 {
                *ret = -(1 as ::core::ffi::c_int);
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
            break;
        } else if *buf as ::core::ffi::c_int == '\n' as i32 {
            buf = buf.offset(1);
            break;
        } else {
            if *num_headers == max_headers {
                *ret = -(1 as ::core::ffi::c_int);
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
            if !(*num_headers != 0 as size_t
                && (*buf as ::core::ffi::c_int == ' ' as i32
                    || *buf as ::core::ffi::c_int == '\t' as i32))
            {
                buf = parse_token(
                    buf,
                    buf_end,
                    &raw mut (*headers.offset(*num_headers as isize)).name,
                    &raw mut (*headers.offset(*num_headers as isize)).name_len,
                    ':' as i32 as ::core::ffi::c_char,
                    ret,
                );
                if buf.is_null() {
                    return ::core::ptr::null::<::core::ffi::c_char>();
                }
                if (*headers.offset(*num_headers as isize)).name_len == 0 as size_t {
                    *ret = -(1 as ::core::ffi::c_int);
                    return ::core::ptr::null::<::core::ffi::c_char>();
                }
                buf = buf.offset(1);
                loop {
                    if buf == buf_end {
                        *ret = -(2 as ::core::ffi::c_int);
                        return ::core::ptr::null::<::core::ffi::c_char>();
                    }
                    if !(*buf as ::core::ffi::c_int == ' ' as i32
                        || *buf as ::core::ffi::c_int == '\t' as i32)
                    {
                        break;
                    }
                    buf = buf.offset(1);
                }
            } else {
                let ref mut fresh11 = (*headers.offset(*num_headers as isize)).name;
                *fresh11 = ::core::ptr::null::<::core::ffi::c_char>();
                (*headers.offset(*num_headers as isize)).name_len = 0 as size_t;
            }
            let mut value: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
            let mut value_len: size_t = 0;
            buf = get_token_to_eol(buf, buf_end, &raw mut value, &raw mut value_len, ret);
            if buf.is_null() {
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
            let mut value_end: *const ::core::ffi::c_char = value.offset(value_len as isize);
            while value_end != value {
                let c: ::core::ffi::c_char = *value_end.offset(-(1 as ::core::ffi::c_int as isize));
                if !(c as ::core::ffi::c_int == ' ' as i32
                    || c as ::core::ffi::c_int == '\t' as i32)
                {
                    break;
                }
                value_end = value_end.offset(-1);
            }
            let ref mut fresh12 = (*headers.offset(*num_headers as isize)).value;
            *fresh12 = value;
            (*headers.offset(*num_headers as isize)).value_len =
                value_end.offset_from(value) as ::core::ffi::c_long as size_t;
            *num_headers = (*num_headers).wrapping_add(1);
        }
    }
    return buf;
}
unsafe extern "C" fn parse_request(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut method: *mut *const ::core::ffi::c_char,
    mut method_len: *mut size_t,
    mut path: *mut *const ::core::ffi::c_char,
    mut path_len: *mut size_t,
    mut minor_version: *mut ::core::ffi::c_int,
    mut headers: *mut phr_header,
    mut num_headers: *mut size_t,
    mut max_headers: size_t,
    mut ret: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    if buf == buf_end {
        *ret = -(2 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    if *buf as ::core::ffi::c_int == '\r' as i32 {
        buf = buf.offset(1);
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        let fresh13 = buf;
        buf = buf.offset(1);
        if *fresh13 as ::core::ffi::c_int != '\n' as i32 {
            *ret = -(1 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
    } else if *buf as ::core::ffi::c_int == '\n' as i32 {
        buf = buf.offset(1);
    }
    buf = parse_token(
        buf,
        buf_end,
        method,
        method_len,
        ' ' as i32 as ::core::ffi::c_char,
        ret,
    );
    if buf.is_null() {
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    loop {
        buf = buf.offset(1);
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        if !(*buf as ::core::ffi::c_int == ' ' as i32) {
            break;
        }
    }
    let mut tok_start: *const ::core::ffi::c_char = buf;
    static mut ranges2: [::core::ffi::c_char; 16] = unsafe {
        ::core::mem::transmute::<[u8; 16], [::core::ffi::c_char; 16]>(
            *b"\0 \x7F\x7F\0\0\0\0\0\0\0\0\0\0\0\0",
        )
    };
    let mut found2: ::core::ffi::c_int = 0;
    buf = findchar_fast(
        buf,
        buf_end,
        &raw const ranges2 as *const ::core::ffi::c_char,
        4 as size_t,
        &raw mut found2,
    );
    if found2 == 0 {
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
    }
    while !(*buf as ::core::ffi::c_int == ' ' as i32) {
        if !((*buf as ::core::ffi::c_uchar as ::core::ffi::c_uint)
            .wrapping_sub(0o40 as ::core::ffi::c_uint)
            < 0o137 as ::core::ffi::c_uint) as ::core::ffi::c_int as ::core::ffi::c_long
            != 0
        {
            if (*buf as ::core::ffi::c_uchar as ::core::ffi::c_int) < ' ' as i32
                || *buf as ::core::ffi::c_int == '\u{7f}' as i32
            {
                *ret = -(1 as ::core::ffi::c_int);
                return ::core::ptr::null::<::core::ffi::c_char>();
            }
        }
        buf = buf.offset(1);
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
    }
    *path = tok_start;
    *path_len = buf.offset_from(tok_start) as ::core::ffi::c_long as size_t;
    loop {
        buf = buf.offset(1);
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        if !(*buf as ::core::ffi::c_int == ' ' as i32) {
            break;
        }
    }
    if *method_len == 0 as size_t || *path_len == 0 as size_t {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    buf = parse_http_version(buf, buf_end, minor_version, ret);
    if buf.is_null() {
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    if *buf as ::core::ffi::c_int == '\r' as i32 {
        buf = buf.offset(1);
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        let fresh14 = buf;
        buf = buf.offset(1);
        if *fresh14 as ::core::ffi::c_int != '\n' as i32 {
            *ret = -(1 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
    } else if *buf as ::core::ffi::c_int == '\n' as i32 {
        buf = buf.offset(1);
    } else {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    return parse_headers(buf, buf_end, headers, num_headers, max_headers, ret);
}
unsafe extern "C" fn phr_parse_request(
    mut buf_start: *const ::core::ffi::c_char,
    mut len: size_t,
    mut method: *mut *const ::core::ffi::c_char,
    mut method_len: *mut size_t,
    mut path: *mut *const ::core::ffi::c_char,
    mut path_len: *mut size_t,
    mut minor_version: *mut ::core::ffi::c_int,
    mut headers: *mut phr_header,
    mut num_headers: *mut size_t,
    mut last_len: size_t,
) -> ::core::ffi::c_int {
    let mut buf: *const ::core::ffi::c_char = buf_start;
    let mut buf_end: *const ::core::ffi::c_char = buf_start.offset(len as isize);
    let mut max_headers: size_t = *num_headers;
    let mut r: ::core::ffi::c_int = 0;
    *method = ::core::ptr::null::<::core::ffi::c_char>();
    *method_len = 0 as size_t;
    *path = ::core::ptr::null::<::core::ffi::c_char>();
    *path_len = 0 as size_t;
    *minor_version = -(1 as ::core::ffi::c_int);
    *num_headers = 0 as size_t;
    if last_len != 0 as size_t && is_complete(buf, buf_end, last_len, &raw mut r).is_null() {
        return r;
    }
    buf = parse_request(
        buf,
        buf_end,
        method,
        method_len,
        path,
        path_len,
        minor_version,
        headers,
        num_headers,
        max_headers,
        &raw mut r,
    );
    if buf.is_null() {
        return r;
    }
    return buf.offset_from(buf_start) as ::core::ffi::c_long as ::core::ffi::c_int;
}
unsafe extern "C" fn parse_response(
    mut buf: *const ::core::ffi::c_char,
    mut buf_end: *const ::core::ffi::c_char,
    mut minor_version: *mut ::core::ffi::c_int,
    mut status: *mut ::core::ffi::c_int,
    mut msg: *mut *const ::core::ffi::c_char,
    mut msg_len: *mut size_t,
    mut headers: *mut phr_header,
    mut num_headers: *mut size_t,
    mut max_headers: size_t,
    mut ret: *mut ::core::ffi::c_int,
) -> *const ::core::ffi::c_char {
    buf = parse_http_version(buf, buf_end, minor_version, ret);
    if buf.is_null() {
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    if *buf as ::core::ffi::c_int != ' ' as i32 {
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    loop {
        buf = buf.offset(1);
        if buf == buf_end {
            *ret = -(2 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
        if !(*buf as ::core::ffi::c_int == ' ' as i32) {
            break;
        }
    }
    if (buf_end.offset_from(buf) as ::core::ffi::c_long) < 4 as ::core::ffi::c_long {
        *ret = -(2 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let mut res_: ::core::ffi::c_int = 0 as ::core::ffi::c_int;
    if (*buf as ::core::ffi::c_int) < '0' as i32 || ('9' as i32) < *buf as ::core::ffi::c_int {
        buf = buf.offset(1);
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh15 = buf;
    buf = buf.offset(1);
    res_ = 100 as ::core::ffi::c_int * (*fresh15 as ::core::ffi::c_int - '0' as i32);
    *status = res_;
    if (*buf as ::core::ffi::c_int) < '0' as i32 || ('9' as i32) < *buf as ::core::ffi::c_int {
        buf = buf.offset(1);
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh16 = buf;
    buf = buf.offset(1);
    res_ = 10 as ::core::ffi::c_int * (*fresh16 as ::core::ffi::c_int - '0' as i32);
    *status += res_;
    if (*buf as ::core::ffi::c_int) < '0' as i32 || ('9' as i32) < *buf as ::core::ffi::c_int {
        buf = buf.offset(1);
        *ret = -(1 as ::core::ffi::c_int);
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    let fresh17 = buf;
    buf = buf.offset(1);
    res_ = 1 as ::core::ffi::c_int * (*fresh17 as ::core::ffi::c_int - '0' as i32);
    *status += res_;
    buf = get_token_to_eol(buf, buf_end, msg, msg_len, ret);
    if buf.is_null() {
        return ::core::ptr::null::<::core::ffi::c_char>();
    }
    if !(*msg_len == 0 as size_t) {
        if **msg as ::core::ffi::c_int == ' ' as i32 {
            loop {
                *msg = (*msg).offset(1);
                *msg_len = (*msg_len).wrapping_sub(1);
                if !(**msg as ::core::ffi::c_int == ' ' as i32) {
                    break;
                }
            }
        } else {
            *ret = -(1 as ::core::ffi::c_int);
            return ::core::ptr::null::<::core::ffi::c_char>();
        }
    }
    return parse_headers(buf, buf_end, headers, num_headers, max_headers, ret);
}
unsafe extern "C" fn phr_parse_response(
    mut buf_start: *const ::core::ffi::c_char,
    mut len: size_t,
    mut minor_version: *mut ::core::ffi::c_int,
    mut status: *mut ::core::ffi::c_int,
    mut msg: *mut *const ::core::ffi::c_char,
    mut msg_len: *mut size_t,
    mut headers: *mut phr_header,
    mut num_headers: *mut size_t,
    mut last_len: size_t,
) -> ::core::ffi::c_int {
    let mut buf: *const ::core::ffi::c_char = buf_start;
    let mut buf_end: *const ::core::ffi::c_char = buf.offset(len as isize);
    let mut max_headers: size_t = *num_headers;
    let mut r: ::core::ffi::c_int = 0;
    *minor_version = -(1 as ::core::ffi::c_int);
    *status = 0 as ::core::ffi::c_int;
    *msg = ::core::ptr::null::<::core::ffi::c_char>();
    *msg_len = 0 as size_t;
    *num_headers = 0 as size_t;
    if last_len != 0 as size_t && is_complete(buf, buf_end, last_len, &raw mut r).is_null() {
        return r;
    }
    buf = parse_response(
        buf,
        buf_end,
        minor_version,
        status,
        msg,
        msg_len,
        headers,
        num_headers,
        max_headers,
        &raw mut r,
    );
    if buf.is_null() {
        return r;
    }
    return buf.offset_from(buf_start) as ::core::ffi::c_long as ::core::ffi::c_int;
}
unsafe extern "C" fn phr_parse_headers(
    mut buf_start: *const ::core::ffi::c_char,
    mut len: size_t,
    mut headers: *mut phr_header,
    mut num_headers: *mut size_t,
    mut last_len: size_t,
) -> ::core::ffi::c_int {
    let mut buf: *const ::core::ffi::c_char = buf_start;
    let mut buf_end: *const ::core::ffi::c_char = buf.offset(len as isize);
    let mut max_headers: size_t = *num_headers;
    let mut r: ::core::ffi::c_int = 0;
    *num_headers = 0 as size_t;
    if last_len != 0 as size_t && is_complete(buf, buf_end, last_len, &raw mut r).is_null() {
        return r;
    }
    buf = parse_headers(buf, buf_end, headers, num_headers, max_headers, &raw mut r);
    if buf.is_null() {
        return r;
    }
    return buf.offset_from(buf_start) as ::core::ffi::c_long as ::core::ffi::c_int;
}
unsafe extern "C" fn decode_hex(mut ch: ::core::ffi::c_int) -> ::core::ffi::c_int {
    if '0' as i32 <= ch && ch <= '9' as i32 {
        return ch - '0' as i32;
    } else if 'A' as i32 <= ch && ch <= 'F' as i32 {
        return ch - 'A' as i32 + 0xa as ::core::ffi::c_int;
    } else if 'a' as i32 <= ch && ch <= 'f' as i32 {
        return ch - 'a' as i32 + 0xa as ::core::ffi::c_int;
    } else {
        return -(1 as ::core::ffi::c_int);
    };
}
unsafe extern "C" fn phr_decode_chunked(
    mut decoder: *mut phr_chunked_decoder,
    mut buf: *mut ::core::ffi::c_char,
    mut _bufsz: *mut size_t,
) -> ssize_t {
    let mut current_block: u64;
    let mut dst: size_t = 0 as size_t;
    let mut src: size_t = 0 as size_t;
    let mut bufsz: size_t = *_bufsz;
    let mut ret: ssize_t = -(2 as ::core::ffi::c_int) as ssize_t;
    (*decoder)._total_read = ((*decoder)._total_read as ::core::ffi::c_ulong)
        .wrapping_add(bufsz as ::core::ffi::c_ulong) as uint64_t
        as uint64_t;
    's_11: loop {
        match (*decoder)._state as ::core::ffi::c_int {
            0 => {
                loop {
                    let mut v: ::core::ffi::c_int = 0;
                    if src == bufsz {
                        current_block = 9852528715560452664;
                        break 's_11;
                    }
                    v = decode_hex(*buf.offset(src as isize) as ::core::ffi::c_int);
                    if v == -(1 as ::core::ffi::c_int) {
                        if (*decoder)._hex_count as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
                            ret = -(1 as ::core::ffi::c_int) as ssize_t;
                            current_block = 9852528715560452664;
                            break 's_11;
                        } else {
                            match *buf.offset(src as isize) as ::core::ffi::c_int {
                                32 | 9 | 59 | 10 | 13 => {
                                    break;
                                }
                                _ => {}
                            }
                            ret = -(1 as ::core::ffi::c_int) as ssize_t;
                            current_block = 9852528715560452664;
                            break 's_11;
                        }
                    } else if (*decoder)._hex_count as ::core::ffi::c_int
                        == (::core::mem::size_of::<size_t>() as usize).wrapping_mul(2 as usize)
                            as ::core::ffi::c_char as ::core::ffi::c_int
                    {
                        ret = -(1 as ::core::ffi::c_int) as ssize_t;
                        current_block = 9852528715560452664;
                        break 's_11;
                    } else {
                        (*decoder).bytes_left_in_chunk = (*decoder)
                            .bytes_left_in_chunk
                            .wrapping_mul(16 as size_t)
                            .wrapping_add(v as size_t);
                        (*decoder)._hex_count += 1;
                        src = src.wrapping_add(1);
                    }
                }
                (*decoder)._hex_count = 0 as ::core::ffi::c_char;
                (*decoder)._state =
                    CHUNKED_IN_CHUNK_EXT as ::core::ffi::c_int as ::core::ffi::c_char;
                current_block = 13242334135786603907;
            }
            1 => {
                current_block = 13242334135786603907;
            }
            2 => {
                current_block = 6886367987138400562;
            }
            3 => {
                current_block = 9520865839495247062;
            }
            4 => {
                current_block = 15549159295339167397;
            }
            5 => {
                current_block = 6427511182945997350;
            }
            6 => {
                loop {
                    if src == bufsz {
                        current_block = 9852528715560452664;
                        break 's_11;
                    }
                    if *buf.offset(src as isize) as ::core::ffi::c_int != '\r' as i32 {
                        break;
                    }
                    src = src.wrapping_add(1);
                }
                let fresh18 = src;
                src = src.wrapping_add(1);
                if *buf.offset(fresh18 as isize) as ::core::ffi::c_int == '\n' as i32 {
                    current_block = 12821564906267970623;
                    break;
                }
                (*decoder)._state =
                    CHUNKED_IN_TRAILERS_LINE_MIDDLE as ::core::ffi::c_int as ::core::ffi::c_char;
                current_block = 2706659501864706830;
            }
            7 => {
                current_block = 2706659501864706830;
            }
            _ => {
                '_c2rust_label: {
                    if (b"decoder is corrupt\0" as *const u8 as *const ::core::ffi::c_char)
                        .is_null()
                    {
                    } else {
                        __assert_fail(
                            b"!\"decoder is corrupt\"\0" as *const u8
                                as *const ::core::ffi::c_char,
                            b"/home/marche/noricum/tests/fixtures/picohttpparser/picohttpparser_combined.c\0"
                                as *const u8 as *const ::core::ffi::c_char,
                            595 as ::core::ffi::c_uint,
                            b"ssize_t phr_decode_chunked(struct phr_chunked_decoder *, char *, size_t *)\0"
                                as *const u8 as *const ::core::ffi::c_char,
                        );
                    }
                };
                continue;
            }
        }
        loop {
            match current_block {
                13242334135786603907 => {
                    if src == bufsz {
                        current_block = 9852528715560452664;
                        break 's_11;
                    }
                    if *buf.offset(src as isize) as ::core::ffi::c_int == '\r' as i32 {
                        src = src.wrapping_add(1);
                        (*decoder)._state = CHUNKED_IN_CHUNK_HEADER_EXPECT_LF as ::core::ffi::c_int
                            as ::core::ffi::c_char;
                        current_block = 6886367987138400562;
                    } else if *buf.offset(src as isize) as ::core::ffi::c_int == '\n' as i32 {
                        ret = -(1 as ::core::ffi::c_int) as ssize_t;
                        current_block = 9852528715560452664;
                        break 's_11;
                    } else {
                        src = src.wrapping_add(1);
                        current_block = 13242334135786603907;
                    }
                }
                9520865839495247062 => {
                    let mut avail: size_t = bufsz.wrapping_sub(src);
                    if avail < (*decoder).bytes_left_in_chunk {
                        if dst != src {
                            memmove(
                                buf.offset(dst as isize) as *mut ::core::ffi::c_void,
                                buf.offset(src as isize) as *const ::core::ffi::c_void,
                                avail,
                            );
                        }
                        src = src.wrapping_add(avail);
                        dst = dst.wrapping_add(avail);
                        (*decoder).bytes_left_in_chunk =
                            (*decoder).bytes_left_in_chunk.wrapping_sub(avail);
                        current_block = 9852528715560452664;
                        break 's_11;
                    } else {
                        if dst != src {
                            memmove(
                                buf.offset(dst as isize) as *mut ::core::ffi::c_void,
                                buf.offset(src as isize) as *const ::core::ffi::c_void,
                                (*decoder).bytes_left_in_chunk,
                            );
                        }
                        src = src.wrapping_add((*decoder).bytes_left_in_chunk);
                        dst = dst.wrapping_add((*decoder).bytes_left_in_chunk);
                        (*decoder).bytes_left_in_chunk = 0 as size_t;
                        (*decoder)._state = CHUNKED_IN_CHUNK_DATA_EXPECT_CR as ::core::ffi::c_int
                            as ::core::ffi::c_char;
                        current_block = 15549159295339167397;
                    }
                }
                6886367987138400562 => {
                    if src == bufsz {
                        current_block = 9852528715560452664;
                        break 's_11;
                    }
                    if *buf.offset(src as isize) as ::core::ffi::c_int != '\n' as i32 {
                        ret = -(1 as ::core::ffi::c_int) as ssize_t;
                        current_block = 9852528715560452664;
                        break 's_11;
                    } else {
                        src = src.wrapping_add(1);
                        if (*decoder).bytes_left_in_chunk == 0 as size_t {
                            if !((*decoder).consume_trailer != 0) {
                                current_block = 12821564906267970623;
                                break 's_11;
                            }
                            (*decoder)._state = CHUNKED_IN_TRAILERS_LINE_HEAD as ::core::ffi::c_int
                                as ::core::ffi::c_char;
                            continue 's_11;
                        } else {
                            (*decoder)._state =
                                CHUNKED_IN_CHUNK_DATA as ::core::ffi::c_int as ::core::ffi::c_char;
                            current_block = 9520865839495247062;
                        }
                    }
                }
                6427511182945997350 => {
                    if src == bufsz {
                        current_block = 9852528715560452664;
                        break 's_11;
                    }
                    if *buf.offset(src as isize) as ::core::ffi::c_int != '\n' as i32 {
                        current_block = 7990025728955927862;
                        break;
                    } else {
                        current_block = 13321564401369230990;
                        break;
                    }
                }
                15549159295339167397 => {
                    if src == bufsz {
                        current_block = 9852528715560452664;
                        break 's_11;
                    }
                    if *buf.offset(src as isize) as ::core::ffi::c_int != '\r' as i32 {
                        ret = -(1 as ::core::ffi::c_int) as ssize_t;
                        current_block = 9852528715560452664;
                        break 's_11;
                    } else {
                        src = src.wrapping_add(1);
                        (*decoder)._state = CHUNKED_IN_CHUNK_DATA_EXPECT_LF as ::core::ffi::c_int
                            as ::core::ffi::c_char;
                        current_block = 6427511182945997350;
                    }
                }
                _ => {
                    if src == bufsz {
                        current_block = 9852528715560452664;
                        break 's_11;
                    }
                    if *buf.offset(src as isize) as ::core::ffi::c_int == '\n' as i32 {
                        src = src.wrapping_add(1);
                        (*decoder)._state = CHUNKED_IN_TRAILERS_LINE_HEAD as ::core::ffi::c_int
                            as ::core::ffi::c_char;
                        continue 's_11;
                    } else {
                        src = src.wrapping_add(1);
                        current_block = 2706659501864706830;
                    }
                }
            }
        }
        match current_block {
            7990025728955927862 => {
                ret = -(1 as ::core::ffi::c_int) as ssize_t;
                current_block = 9852528715560452664;
                break;
            }
            _ => {
                src = src.wrapping_add(1);
                (*decoder)._state =
                    CHUNKED_IN_CHUNK_SIZE as ::core::ffi::c_int as ::core::ffi::c_char;
            }
        }
    }
    match current_block {
        12821564906267970623 => {
            ret = bufsz.wrapping_sub(src) as ssize_t;
        }
        _ => {}
    }
    if dst != src {
        memmove(
            buf.offset(dst as isize) as *mut ::core::ffi::c_void,
            buf.offset(src as isize) as *const ::core::ffi::c_void,
            bufsz.wrapping_sub(src),
        );
    }
    *_bufsz = dst;
    if ret == -(2 as ::core::ffi::c_int) as ssize_t {
        (*decoder)._total_overhead = ((*decoder)._total_overhead as ::core::ffi::c_ulong)
            .wrapping_add(bufsz.wrapping_sub(dst) as ::core::ffi::c_ulong)
            as uint64_t as uint64_t;
        if (*decoder)._total_overhead
            >= (100 as ::core::ffi::c_int * 1024 as ::core::ffi::c_int) as uint64_t
            && (*decoder)
                ._total_read
                .wrapping_sub((*decoder)._total_overhead)
                < (*decoder)._total_read.wrapping_div(4 as uint64_t)
        {
            ret = -(1 as ::core::ffi::c_int) as ssize_t;
        }
    }
    return ret;
}
unsafe extern "C" fn bufis(
    mut s: *const ::core::ffi::c_char,
    mut l: size_t,
    mut t: *const ::core::ffi::c_char,
) -> ::core::ffi::c_int {
    return (strlen(t) == l
        && memcmp(
            s as *const ::core::ffi::c_void,
            t as *const ::core::ffi::c_void,
            l,
        ) == 0 as ::core::ffi::c_int) as ::core::ffi::c_int;
}
pub const INPUT_BUF_SIZE: ::core::ffi::c_int = 4096 as ::core::ffi::c_int;
static mut input_storage: [::core::ffi::c_char; 4096] = [0; 4096];
static mut inputbuf: *mut ::core::ffi::c_char =
    ::core::ptr::null::<::core::ffi::c_char>() as *mut ::core::ffi::c_char;
unsafe extern "C" fn test_request() {
    let mut method: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut method_len: size_t = 0;
    let mut path: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut path_len: size_t = 0;
    let mut minor_version: ::core::ffi::c_int = 0;
    let mut headers: [phr_header; 4] = [phr_header {
        name: ::core::ptr::null::<::core::ffi::c_char>(),
        name_len: 0,
        value: ::core::ptr::null::<::core::ffi::c_char>(),
        value_len: 0,
    }; 4];
    let mut num_headers: size_t = 0;
    let mut slen: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t).wrapping_sub(1 as size_t);
    note(b"simple\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen as isize)),
            slen,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 0 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            method,
            method_len,
            b"GET\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            path,
            path_len,
            b"/\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_0: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 18]>() as size_t).wrapping_sub(1 as size_t);
    note(b"partial\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_0 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\n\r\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_0,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_0 as isize)),
            slen_0,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_0 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_1: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 52]>() as size_t).wrapping_sub(1 as size_t);
    note(b"parse headers\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_1 as isize)) as *mut ::core::ffi::c_void,
        b"GET /hoge HTTP/1.1\r\nHost: example.com\r\nCookie: \r\n\r\n\0" as *const u8
            as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_1,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_1 as isize)),
            slen_1,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_1 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 2 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            method,
            method_len,
            b"GET\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            path,
            path_len,
            b"/hoge\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 1 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"Host\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"example.com\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].name,
            headers[1 as ::core::ffi::c_int as usize].name_len,
            b"Cookie\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].value,
            headers[1 as ::core::ffi::c_int as usize].value_len,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_2: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 64]>() as size_t).wrapping_sub(1 as size_t);
    note(b"multibyte included\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_2 as isize)) as *mut ::core::ffi::c_void,
        b"GET /hoge HTTP/1.1\r\nHost: example.com\r\nUser-Agent: \xE3\x81\xB2\xE3/1.0\r\n\r\n\0"
            as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_2,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_2 as isize)),
            slen_2,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_2 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 2 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            method,
            method_len,
            b"GET\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            path,
            path_len,
            b"/hoge\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 1 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"Host\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"example.com\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].name,
            headers[1 as ::core::ffi::c_int as usize].name_len,
            b"User-Agent\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].value,
            headers[1 as ::core::ffi::c_int as usize].value_len,
            b"\xE3\x81\xB2\xE3/1.0\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_3: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 40]>() as size_t).wrapping_sub(1 as size_t);
    note(b"parse multiline\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_3 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\nfoo: \r\nfoo: b\r\n  \tc\r\n\r\n\0" as *const u8
            as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_3,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_3 as isize)),
            slen_3,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_3 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 3 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            method,
            method_len,
            b"GET\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            path,
            path_len,
            b"/\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"foo\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].name,
            headers[1 as ::core::ffi::c_int as usize].name_len,
            b"foo\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].value,
            headers[1 as ::core::ffi::c_int as usize].value_len,
            b"b\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (headers[2 as ::core::ffi::c_int as usize].name
            == ::core::ptr::null_mut::<::core::ffi::c_void>() as *const ::core::ffi::c_char)
            as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[2 as ::core::ffi::c_int as usize].value,
            headers[2 as ::core::ffi::c_int as usize].value_len,
            b"  \tc\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_4: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 29]>() as size_t).wrapping_sub(1 as size_t);
    note(b"parse header name with trailing space\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_4 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\nfoo : ab\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_4,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_4 as isize)),
            slen_4,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_4 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_5: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 4]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 1\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_5 as isize)) as *mut ::core::ffi::c_void,
        b"GET\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_5,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_5 as isize)),
            slen_5,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_5 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (method == ::core::ptr::null_mut::<::core::ffi::c_void>() as *const ::core::ffi::c_char)
            as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_6: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 5]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 2\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_6 as isize)) as *mut ::core::ffi::c_void,
        b"GET \0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_6,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_6 as isize)),
            slen_6,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_6 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            method,
            method_len,
            b"GET\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_7: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 6]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 3\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_7 as isize)) as *mut ::core::ffi::c_void,
        b"GET /\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_7,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_7 as isize)),
            slen_7,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_7 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (path == ::core::ptr::null_mut::<::core::ffi::c_void>() as *const ::core::ffi::c_char)
            as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_8: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 7]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 4\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_8 as isize)) as *mut ::core::ffi::c_void,
        b"GET / \0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_8,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_8 as isize)),
            slen_8,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_8 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            path,
            path_len,
            b"/\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_9: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 8]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 5\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_9 as isize)) as *mut ::core::ffi::c_void,
        b"GET / H\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_9,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_9 as isize)),
            slen_9,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_9 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_10: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 14]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 6\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_10 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_10,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_10 as isize)),
            slen_10,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_10 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_11: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 15]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 7\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_11 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_11,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_11 as isize)),
            slen_11,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_11 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == -(1 as ::core::ffi::c_int)) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_12: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 8\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_12 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_12,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_12 as isize)),
            slen_12,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_12 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_13: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 22]>() as size_t).wrapping_sub(1 as size_t);
    note(b"slowloris (incomplete)\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_13 as isize)) as *mut ::core::ffi::c_void,
        b"GET /hoge HTTP/1.0\r\n\r\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_13,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_13 as isize)),
            slen_13,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            strlen(b"GET /hoge HTTP/1.0\r\n\r\0" as *const u8 as *const ::core::ffi::c_char)
                .wrapping_sub(1 as size_t),
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_13 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_14: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 23]>() as size_t).wrapping_sub(1 as size_t);
    note(b"slowloris (complete)\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_14 as isize)) as *mut ::core::ffi::c_void,
        b"GET /hoge HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_14,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_14 as isize)),
            slen_14,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            strlen(b"GET /hoge HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char)
                .wrapping_sub(1 as size_t),
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_14 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_15: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 16]>() as size_t).wrapping_sub(1 as size_t);
    note(b"empty method\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_15 as isize)) as *mut ::core::ffi::c_void,
        b" / HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_15,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_15 as isize)),
            slen_15,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_15 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_16: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 18]>() as size_t).wrapping_sub(1 as size_t);
    note(b"empty request-target\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_16 as isize)) as *mut ::core::ffi::c_void,
        b"GET  HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_16,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_16 as isize)),
            slen_16,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_16 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_17: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 23]>() as size_t).wrapping_sub(1 as size_t);
    note(b"empty header name\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_17 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\n:a\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_17,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_17 as isize)),
            slen_17,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_17 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_18: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 24]>() as size_t).wrapping_sub(1 as size_t);
    note(b"header name (space only)\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_18 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\n :a\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_18,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_18 as isize)),
            slen_18,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_18 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_19: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t).wrapping_sub(1 as size_t);
    note(b"NUL in method\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_19 as isize)) as *mut ::core::ffi::c_void,
        b"G\0T / HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_19,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_19 as isize)),
            slen_19,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_19 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_20: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t).wrapping_sub(1 as size_t);
    note(b"tab in method\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_20 as isize)) as *mut ::core::ffi::c_void,
        b"G\tT / HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_20,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_20 as isize)),
            slen_20,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_20 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_21: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 20]>() as size_t).wrapping_sub(1 as size_t);
    note(b"invalid method\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_21 as isize)) as *mut ::core::ffi::c_void,
        b":GET / HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_21,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_21 as isize)),
            slen_21,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_21 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_22: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 25]>() as size_t).wrapping_sub(1 as size_t);
    note(b"DEL in uri-path\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_22 as isize)) as *mut ::core::ffi::c_void,
        b"GET /\x7Fhello HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_22,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_22 as isize)),
            slen_22,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_22 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_23: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 27]>() as size_t).wrapping_sub(1 as size_t);
    note(b"NUL in header name\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_23 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\na\0b: c\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_23,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_23 as isize)),
            slen_23,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_23 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_24: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 28]>() as size_t).wrapping_sub(1 as size_t);
    note(b"NUL in header value\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_24 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\nab: c\0d\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_24,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_24 as isize)),
            slen_24,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_24 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_25: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 27]>() as size_t).wrapping_sub(1 as size_t);
    note(b"CTL in header name\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_25 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\na\x1Bb: c\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_25,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_25 as isize)),
            slen_25,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_25 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_26: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 27]>() as size_t).wrapping_sub(1 as size_t);
    note(b"CTL in header value\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_26 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\nab: c\x1B\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_26,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_26 as isize)),
            slen_26,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_26 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_27: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 25]>() as size_t).wrapping_sub(1 as size_t);
    note(b"invalid char in header value\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_27 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\n/: 1\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_27,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_27 as isize)),
            slen_27,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_27 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_28: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 28]>() as size_t).wrapping_sub(1 as size_t);
    note(b"accept MSB chars\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_28 as isize)) as *mut ::core::ffi::c_void,
        b"GET /\xA0 HTTP/1.0\r\nh: c\xA2y\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_28,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_28 as isize)),
            slen_28,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_28 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 1 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            method,
            method_len,
            b"GET\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            path,
            path_len,
            b"/\xA0\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"h\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"c\xA2y\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_29: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 26]>() as size_t).wrapping_sub(1 as size_t);
    note(b"accept |~ (though forbidden by SSE)\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_29 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\n|~: 1\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_29,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_29 as isize)),
            slen_29,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_29 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 1 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"|~\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"1\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_30: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 25]>() as size_t).wrapping_sub(1 as size_t);
    note(b"disallow {\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_30 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\n{: 1\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_30,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_30 as isize)),
            slen_30,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_30 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_31: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 30]>() as size_t).wrapping_sub(1 as size_t);
    note(
        b"exclude leading and trailing spaces in header value\0" as *const u8
            as *const ::core::ffi::c_char,
    );
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_31 as isize)) as *mut ::core::ffi::c_void,
        b"GET / HTTP/1.0\r\nfoo: a \t \r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_31,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_31 as isize)),
            slen_31,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_31 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"a\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_32: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 23]>() as size_t).wrapping_sub(1 as size_t);
    note(b"accept multiple spaces between tokens\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_32 as isize)) as *mut ::core::ffi::c_void,
        b"GET   /   HTTP/1.0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_32,
    );
    _ok(
        (phr_parse_request(
            inputbuf.offset(-(slen_32 as isize)),
            slen_32,
            &raw mut method,
            &raw mut method_len,
            &raw mut path,
            &raw mut path_len,
            &raw mut minor_version,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_32 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
}
unsafe extern "C" fn test_response() {
    let mut minor_version: ::core::ffi::c_int = 0;
    let mut status: ::core::ffi::c_int = 0;
    let mut msg: *const ::core::ffi::c_char = ::core::ptr::null::<::core::ffi::c_char>();
    let mut msg_len: size_t = 0;
    let mut headers: [phr_header; 4] = [phr_header {
        name: ::core::ptr::null::<::core::ffi::c_char>(),
        name_len: 0,
        value: ::core::ptr::null::<::core::ffi::c_char>(),
        value_len: 0,
    }; 4];
    let mut num_headers: size_t = 0;
    let mut slen: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 20]>() as size_t).wrapping_sub(1 as size_t);
    note(b"simple\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.0 200 OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen as isize)),
            slen,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 0 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (status == 200 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            msg,
            msg_len,
            b"OK\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_0: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t).wrapping_sub(1 as size_t);
    note(b"partial\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_0 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.0 200 OK\r\n\r\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_0,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_0 as isize)),
            slen_0,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_0 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_1: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 49]>() as size_t).wrapping_sub(1 as size_t);
    note(b"parse headers\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_1 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 OK\r\nHost: example.com\r\nCookie: \r\n\r\n\0" as *const u8
            as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_1,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_1 as isize)),
            slen_1,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_1 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 2 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 1 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (status == 200 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            msg,
            msg_len,
            b"OK\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"Host\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"example.com\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].name,
            headers[1 as ::core::ffi::c_int as usize].name_len,
            b"Cookie\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].value,
            headers[1 as ::core::ffi::c_int as usize].value_len,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_2: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 41]>() as size_t).wrapping_sub(1 as size_t);
    note(b"parse multiline\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_2 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.0 200 OK\r\nfoo: \r\nfoo: b\r\n  \tc\r\n\r\n\0" as *const u8
            as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_2,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_2 as isize)),
            slen_2,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_2 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 3 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (status == 200 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            msg,
            msg_len,
            b"OK\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"foo\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].name,
            headers[1 as ::core::ffi::c_int as usize].name_len,
            b"foo\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].value,
            headers[1 as ::core::ffi::c_int as usize].value_len,
            b"b\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (headers[2 as ::core::ffi::c_int as usize].name
            == ::core::ptr::null_mut::<::core::ffi::c_void>() as *const ::core::ffi::c_char)
            as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[2 as ::core::ffi::c_int as usize].value,
            headers[2 as ::core::ffi::c_int as usize].value_len,
            b"  \tc\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_3: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 39]>() as size_t).wrapping_sub(1 as size_t);
    note(b"internal server error\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_3 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.0 500 Internal Server Error\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_3,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_3 as isize)),
            slen_3,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_3 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 0 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (status == 500 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            msg,
            msg_len,
            b"Internal Server Error\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (msg_len
            == (::core::mem::size_of::<[::core::ffi::c_char; 22]>() as usize)
                .wrapping_sub(1 as usize)) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_4: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 2]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 1\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_4 as isize)) as *mut ::core::ffi::c_void,
        b"H\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_4,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_4 as isize)),
            slen_4,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_4 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_5: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 8]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 2\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_5 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_5,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_5 as isize)),
            slen_5,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_5 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_6: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 9]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 3\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_6 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_6,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_6 as isize)),
            slen_6,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_6 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == -(1 as ::core::ffi::c_int)) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_7: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 10]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 4\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_7 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 \0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_7,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_7 as isize)),
            slen_7,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_7 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (minor_version == 1 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_8: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 11]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 5\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_8 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 2\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_8,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_8 as isize)),
            slen_8,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_8 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_9: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 13]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 6\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_9 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_9,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_9 as isize)),
            slen_9,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_9 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (status == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_10: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 14]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 7\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_10 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 \0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
        slen_10,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_10 as isize)),
            slen_10,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_10 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (status == 200 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_11: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 15]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 8\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_11 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 O\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_11,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_11 as isize)),
            slen_11,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_11 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_12: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 17]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 9\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_12 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 OK\r\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_12,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_12 as isize)),
            slen_12,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_12 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (msg == ::core::ptr::null_mut::<::core::ffi::c_void>() as *const ::core::ffi::c_char)
            as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_13: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 18]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 10\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_13 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 OK\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_13,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_13 as isize)),
            slen_13,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_13 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            msg,
            msg_len,
            b"OK\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_14: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 17]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 11\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_14 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 OK\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_14,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_14 as isize)),
            slen_14,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_14 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            msg,
            msg_len,
            b"OK\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_15: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 23]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 11\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_15 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 OK\r\nA: 1\r\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_15,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_15 as isize)),
            slen_15,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_15 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 0 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_16: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 24]>() as size_t).wrapping_sub(1 as size_t);
    note(b"incomplete 12\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_16 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 OK\r\nA: 1\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_16,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_16 as isize)),
            slen_16,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_16 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 1 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"A\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"1\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_17: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t).wrapping_sub(1 as size_t);
    note(b"slowloris (incomplete)\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_17 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.0 200 OK\r\n\r\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_17,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_17 as isize)),
            slen_17,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            strlen(b"HTTP/1.0 200 OK\r\n\r\0" as *const u8 as *const ::core::ffi::c_char)
                .wrapping_sub(1 as size_t),
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_17 as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_18: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 20]>() as size_t).wrapping_sub(1 as size_t);
    note(b"slowloris (complete)\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_18 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.0 200 OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_18,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_18 as isize)),
            slen_18,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            strlen(b"HTTP/1.0 200 OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char)
                .wrapping_sub(1 as size_t),
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_18 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_19: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t).wrapping_sub(1 as size_t);
    note(b"invalid http version\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_19 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1. 200 OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_19,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_19 as isize)),
            slen_19,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_19 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_20: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 21]>() as size_t).wrapping_sub(1 as size_t);
    note(b"invalid http version 2\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_20 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.2z 200 OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_20,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_20 as isize)),
            slen_20,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_20 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_21: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 17]>() as size_t).wrapping_sub(1 as size_t);
    note(b"no status code\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_21 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1  OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_21,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_21 as isize)),
            slen_21,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_21 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_22: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 17]>() as size_t).wrapping_sub(1 as size_t);
    note(
        b"accept missing trailing whitespace in status-line\0" as *const u8
            as *const ::core::ffi::c_char,
    );
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_22 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_22,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_22 as isize)),
            slen_22,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_22 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            msg,
            msg_len,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_23: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 18]>() as size_t).wrapping_sub(1 as size_t);
    note(b"garbage after status 1\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_23 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200X\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_23,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_23 as isize)),
            slen_23,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_23 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_24: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t).wrapping_sub(1 as size_t);
    note(b"garbage after status 2\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_24 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200X \r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_24,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_24 as isize)),
            slen_24,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_24 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_25: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 21]>() as size_t).wrapping_sub(1 as size_t);
    note(b"garbage after status 3\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_25 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200X OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_25,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_25 as isize)),
            slen_25,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            slen_25 as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_26: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 33]>() as size_t).wrapping_sub(1 as size_t);
    note(
        b"exclude leading and trailing spaces in header value\0" as *const u8
            as *const ::core::ffi::c_char,
    );
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_26 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1 200 OK\r\nbar: \t b\t \t\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_26,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_26 as isize)),
            slen_26,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_26 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"b\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    let mut slen_27: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 24]>() as size_t).wrapping_sub(1 as size_t);
    note(b"accept multiple spaces between tokens\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    memcpy(
        inputbuf.offset(-(slen_27 as isize)) as *mut ::core::ffi::c_void,
        b"HTTP/1.1   200   OK\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
            as *const ::core::ffi::c_void,
        slen_27,
    );
    _ok(
        (phr_parse_response(
            inputbuf.offset(-(slen_27 as isize)),
            slen_27,
            &raw mut minor_version,
            &raw mut status,
            &raw mut msg,
            &raw mut msg_len,
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            slen_27 as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
}
unsafe extern "C" fn test_headers() {
    let mut headers: [phr_header; 4] = [phr_header {
        name: ::core::ptr::null::<::core::ffi::c_char>(),
        name_len: 0,
        value: ::core::ptr::null::<::core::ffi::c_char>(),
        value_len: 0,
    }; 4];
    let mut num_headers: size_t = 0;
    note(b"simple\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    _ok(
        (phr_parse_headers(
            b"Host: example.com\r\nCookie: \r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            strlen(
                b"Host: example.com\r\nCookie: \r\n\r\n\0" as *const u8
                    as *const ::core::ffi::c_char,
            ),
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            strlen(
                b"Host: example.com\r\nCookie: \r\n\r\n\0" as *const u8
                    as *const ::core::ffi::c_char,
            ) as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 2 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"Host\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"example.com\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].name,
            headers[1 as ::core::ffi::c_int as usize].name_len,
            b"Cookie\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].value,
            headers[1 as ::core::ffi::c_int as usize].value_len,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    note(b"slowloris\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    _ok(
        (phr_parse_headers(
            b"Host: example.com\r\nCookie: \r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            strlen(
                b"Host: example.com\r\nCookie: \r\n\r\n\0" as *const u8
                    as *const ::core::ffi::c_char,
            ),
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            1 as size_t,
        ) == (if 0 as ::core::ffi::c_int == 0 as ::core::ffi::c_int {
            strlen(
                b"Host: example.com\r\nCookie: \r\n\r\n\0" as *const u8
                    as *const ::core::ffi::c_char,
            ) as ::core::ffi::c_int
        } else {
            0 as ::core::ffi::c_int
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (num_headers == 2 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].name,
            headers[0 as ::core::ffi::c_int as usize].name_len,
            b"Host\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[0 as ::core::ffi::c_int as usize].value,
            headers[0 as ::core::ffi::c_int as usize].value_len,
            b"example.com\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].name,
            headers[1 as ::core::ffi::c_int as usize].name_len,
            b"Cookie\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(
            headers[1 as ::core::ffi::c_int as usize].value,
            headers[1 as ::core::ffi::c_int as usize].value_len,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    note(b"partial\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    _ok(
        (phr_parse_headers(
            b"Host: example.com\r\nCookie: \r\n\r\0" as *const u8 as *const ::core::ffi::c_char,
            strlen(
                b"Host: example.com\r\nCookie: \r\n\r\0" as *const u8 as *const ::core::ffi::c_char,
            ),
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(2 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            strlen(
                b"Host: example.com\r\nCookie: \r\n\r\0" as *const u8 as *const ::core::ffi::c_char,
            ) as ::core::ffi::c_int
        } else {
            -(2 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    note(b"error\0" as *const u8 as *const ::core::ffi::c_char);
    num_headers = (::core::mem::size_of::<[phr_header; 4]>() as usize)
        .wrapping_div(::core::mem::size_of::<phr_header>() as usize) as size_t;
    _ok(
        (phr_parse_headers(
            b"Host: e\x07fample.com\r\nCookie: \r\n\r\0" as *const u8 as *const ::core::ffi::c_char,
            strlen(
                b"Host: e\x07fample.com\r\nCookie: \r\n\r\0" as *const u8
                    as *const ::core::ffi::c_char,
            ),
            &raw mut headers as *mut phr_header,
            &raw mut num_headers,
            0 as size_t,
        ) == (if -(1 as ::core::ffi::c_int) == 0 as ::core::ffi::c_int {
            strlen(
                b"Host: e\x07fample.com\r\nCookie: \r\n\r\0" as *const u8
                    as *const ::core::ffi::c_char,
            ) as ::core::ffi::c_int
        } else {
            -(1 as ::core::ffi::c_int)
        })) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
}
unsafe extern "C" fn test_chunked_at_once(
    mut line: ::core::ffi::c_int,
    mut consume_trailer: ::core::ffi::c_int,
    mut encoded: *const ::core::ffi::c_char,
    mut decoded: *const ::core::ffi::c_char,
    mut expected: ssize_t,
) {
    let mut dec: phr_chunked_decoder = phr_chunked_decoder {
        bytes_left_in_chunk: 0 as size_t,
        consume_trailer: 0,
        _hex_count: 0,
        _state: 0,
        _total_read: 0,
        _total_overhead: 0,
    };
    let mut buf: *mut ::core::ffi::c_char = ::core::ptr::null_mut::<::core::ffi::c_char>();
    let mut bufsz: size_t = 0;
    let mut ret: ssize_t = 0;
    dec.consume_trailer = consume_trailer as ::core::ffi::c_char;
    note(b"testing at-once\0" as *const u8 as *const ::core::ffi::c_char);
    buf = strdup(encoded);
    bufsz = strlen(buf);
    ret = phr_decode_chunked(&raw mut dec, buf, &raw mut bufsz);
    _ok(
        (ret == expected) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (bufsz == strlen(decoded)) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        bufis(buf, bufsz, decoded),
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    if expected >= 0 as ssize_t {
        if ret == expected {
            _ok(
                bufis(
                    buf.offset(bufsz as isize),
                    ret as size_t,
                    encoded
                        .offset(strlen(encoded) as isize)
                        .offset(-(ret as isize)),
                ),
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
        } else {
            _ok(
                0 as ::core::ffi::c_int,
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
        }
    }
    free(buf as *mut ::core::ffi::c_void);
}
unsafe extern "C" fn test_chunked_per_byte(
    mut line: ::core::ffi::c_int,
    mut consume_trailer: ::core::ffi::c_int,
    mut encoded: *const ::core::ffi::c_char,
    mut decoded: *const ::core::ffi::c_char,
    mut expected: ssize_t,
) {
    let mut current_block: u64;
    let mut dec: phr_chunked_decoder = phr_chunked_decoder {
        bytes_left_in_chunk: 0 as size_t,
        consume_trailer: 0,
        _hex_count: 0,
        _state: 0,
        _total_read: 0,
        _total_overhead: 0,
    };
    let mut buf: *mut ::core::ffi::c_char =
        malloc(strlen(encoded).wrapping_add(1 as size_t)) as *mut ::core::ffi::c_char;
    let mut bytes_to_consume: size_t = strlen(encoded).wrapping_sub(
        (if expected >= 0 as ssize_t {
            expected
        } else {
            0 as ssize_t
        }) as size_t,
    );
    let mut bytes_ready: size_t = 0 as size_t;
    let mut bufsz: size_t = 0;
    let mut i: size_t = 0;
    let mut ret: ssize_t = 0;
    dec.consume_trailer = consume_trailer as ::core::ffi::c_char;
    note(b"testing per-byte\0" as *const u8 as *const ::core::ffi::c_char);
    i = 0 as size_t;
    loop {
        if !(i < bytes_to_consume.wrapping_sub(1 as size_t)) {
            current_block = 11812396948646013369;
            break;
        }
        *buf.offset(bytes_ready as isize) = *encoded.offset(i as isize);
        bufsz = 1 as size_t;
        ret = phr_decode_chunked(
            &raw mut dec,
            buf.offset(bytes_ready as isize),
            &raw mut bufsz,
        );
        if ret != -(2 as ::core::ffi::c_int) as ssize_t {
            _ok(
                0 as ::core::ffi::c_int,
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
            current_block = 3919948485575641400;
            break;
        } else {
            bytes_ready = bytes_ready.wrapping_add(bufsz);
            i = i.wrapping_add(1);
        }
    }
    match current_block {
        11812396948646013369 => {
            strcpy(
                buf.offset(bytes_ready as isize),
                encoded
                    .offset(bytes_to_consume as isize)
                    .offset(-(1 as ::core::ffi::c_int as isize)),
            );
            bufsz = strlen(buf.offset(bytes_ready as isize));
            ret = phr_decode_chunked(
                &raw mut dec,
                buf.offset(bytes_ready as isize),
                &raw mut bufsz,
            );
            _ok(
                (ret == expected) as ::core::ffi::c_int,
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
            bytes_ready = bytes_ready.wrapping_add(bufsz);
            _ok(
                (bytes_ready == strlen(decoded)) as ::core::ffi::c_int,
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
            _ok(
                bufis(buf, bytes_ready, decoded),
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
            if expected >= 0 as ssize_t {
                if ret == expected {
                    _ok(
                        bufis(
                            buf.offset(bytes_ready as isize),
                            expected as size_t,
                            encoded.offset(bytes_to_consume as isize),
                        ),
                        b"check\0" as *const u8 as *const ::core::ffi::c_char,
                    );
                } else {
                    _ok(
                        0 as ::core::ffi::c_int,
                        b"check\0" as *const u8 as *const ::core::ffi::c_char,
                    );
                }
            }
        }
        _ => {}
    }
    free(buf as *mut ::core::ffi::c_void);
}
unsafe extern "C" fn test_chunked_failure(
    mut line: ::core::ffi::c_int,
    mut encoded: *const ::core::ffi::c_char,
    mut expected: ssize_t,
) {
    let mut current_block: u64;
    let mut dec: phr_chunked_decoder = phr_chunked_decoder {
        bytes_left_in_chunk: 0 as size_t,
        consume_trailer: 0,
        _hex_count: 0,
        _state: 0,
        _total_read: 0,
        _total_overhead: 0,
    };
    let mut buf: *mut ::core::ffi::c_char = strdup(encoded);
    let mut bufsz: size_t = 0;
    let mut i: size_t = 0;
    let mut ret: ssize_t = 0;
    note(b"testing failure at-once\0" as *const u8 as *const ::core::ffi::c_char);
    bufsz = strlen(buf);
    ret = phr_decode_chunked(&raw mut dec, buf, &raw mut bufsz);
    _ok(
        (ret == expected) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    note(b"testing failure per-byte\0" as *const u8 as *const ::core::ffi::c_char);
    memset(
        &raw mut dec as *mut ::core::ffi::c_void,
        0 as ::core::ffi::c_int,
        ::core::mem::size_of::<phr_chunked_decoder>() as size_t,
    );
    i = 0 as size_t;
    loop {
        if !(*encoded.offset(i as isize) as ::core::ffi::c_int != '\0' as i32) {
            current_block = 10048703153582371463;
            break;
        }
        *buf.offset(0 as ::core::ffi::c_int as isize) = *encoded.offset(i as isize);
        bufsz = 1 as size_t;
        ret = phr_decode_chunked(&raw mut dec, buf, &raw mut bufsz);
        if ret == -(1 as ::core::ffi::c_int) as ssize_t {
            _ok(
                (ret == expected) as ::core::ffi::c_int,
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
            current_block = 3040373654948384620;
            break;
        } else if ret == -(2 as ::core::ffi::c_int) as ssize_t {
            i = i.wrapping_add(1);
        } else {
            _ok(
                0 as ::core::ffi::c_int,
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
            current_block = 3040373654948384620;
            break;
        }
    }
    match current_block {
        10048703153582371463 => {
            _ok(
                (ret == expected) as ::core::ffi::c_int,
                b"check\0" as *const u8 as *const ::core::ffi::c_char,
            );
        }
        _ => {}
    }
    free(buf as *mut ::core::ffi::c_void);
}
static mut chunked_test_runners: [Option<
    unsafe extern "C" fn(
        ::core::ffi::c_int,
        ::core::ffi::c_int,
        *const ::core::ffi::c_char,
        *const ::core::ffi::c_char,
        ssize_t,
    ) -> (),
>; 3] = unsafe {
    [
        Some(
            test_chunked_at_once
                as unsafe extern "C" fn(
                    ::core::ffi::c_int,
                    ::core::ffi::c_int,
                    *const ::core::ffi::c_char,
                    *const ::core::ffi::c_char,
                    ssize_t,
                ) -> (),
        ),
        Some(
            test_chunked_per_byte
                as unsafe extern "C" fn(
                    ::core::ffi::c_int,
                    ::core::ffi::c_int,
                    *const ::core::ffi::c_char,
                    *const ::core::ffi::c_char,
                    ssize_t,
                ) -> (),
        ),
        None,
    ]
};
unsafe extern "C" fn test_chunked() {
    let mut i: size_t = 0;
    i = 0 as size_t;
    while chunked_test_runners[i as usize].is_some() {
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            994 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            b"b\r\nhello world\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            995 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            b"6\r\nhello \r\n5\r\nworld\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            996 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            b"6;comment=hi\r\nhello \r\n5\r\nworld\r\n0\r\n\0" as *const u8
                as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            997 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            b"6 ; comment\r\nhello \r\n5\r\nworld\r\n0\r\n\0" as *const u8
                as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            998 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            b"6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\r\nc: d\r\n\r\n\0" as *const u8
                as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            (::core::mem::size_of::<[::core::ffi::c_char; 15]>() as usize).wrapping_sub(1 as usize)
                as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1000 as ::core::ffi::c_int,
            0 as ::core::ffi::c_int,
            b"b\r\nhello world\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        i = i.wrapping_add(1);
    }
    note(b"failures\0" as *const u8 as *const ::core::ffi::c_char);
    test_chunked_failure(
        1004 as ::core::ffi::c_int,
        b"z\r\nabcdefg\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
    if ::core::mem::size_of::<size_t>() as usize == 8 as usize {
        test_chunked_failure(
            1006 as ::core::ffi::c_int,
            b"6\r\nhello \r\nffffffffffffffff\r\nabcdefg\0" as *const u8
                as *const ::core::ffi::c_char,
            -(2 as ::core::ffi::c_int) as ssize_t,
        );
        test_chunked_failure(
            1007 as ::core::ffi::c_int,
            b"6\r\nhello \r\nfffffffffffffffff\r\nabcdefg\0" as *const u8
                as *const ::core::ffi::c_char,
            -(1 as ::core::ffi::c_int) as ssize_t,
        );
    }
    test_chunked_failure(
        1009 as ::core::ffi::c_int,
        b"1x\r\na\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
    test_chunked_failure(
        1011 as ::core::ffi::c_int,
        b"6\nhello \r\n5\r\nworld\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
    test_chunked_failure(
        1012 as ::core::ffi::c_int,
        b"6\r\nhello \n5\r\nworld\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
    test_chunked_failure(
        1013 as ::core::ffi::c_int,
        b"6\r\nhello \r\n5\r\nworld\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
    test_chunked_failure(
        1014 as ::core::ffi::c_int,
        b"6\r\nhello \r\n5\r\nworld\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
    test_chunked_failure(
        1015 as ::core::ffi::c_int,
        b"6\r\nhello \r\n5\r\nworld\r\n0\n\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
    test_chunked_failure(
        1016 as ::core::ffi::c_int,
        b"6\rX\nhello \n5\r\nworld\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
        -(1 as ::core::ffi::c_int) as ssize_t,
    );
}
unsafe extern "C" fn test_chunked_consume_trailer() {
    let mut i: size_t = 0;
    i = 0 as size_t;
    while chunked_test_runners[i as usize].is_some() {
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1022 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            b"b\r\nhello world\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            -(2 as ::core::ffi::c_int) as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1023 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            b"6\r\nhello \r\n5\r\nworld\r\n0\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            -(2 as ::core::ffi::c_int) as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1024 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            b"6;comment=hi\r\nhello \r\n5\r\nworld\r\n0\r\n\0" as *const u8
                as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            -(2 as ::core::ffi::c_int) as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1025 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            b"b\r\nhello world\r\n0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1026 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            b"6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\r\nc: d\r\n\r\n\0" as *const u8
                as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1027 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            b"b\r\nhello world\r\n0\r\n\n\0" as *const u8 as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        chunked_test_runners[i as usize].expect("non-null function pointer")(
            1028 as ::core::ffi::c_int,
            1 as ::core::ffi::c_int,
            b"6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\nc: d\n\n\0" as *const u8
                as *const ::core::ffi::c_char,
            b"hello world\0" as *const u8 as *const ::core::ffi::c_char,
            0 as ssize_t,
        );
        i = i.wrapping_add(1);
    }
}
unsafe extern "C" fn test_chunked_leftdata() {
    let mut dec: phr_chunked_decoder = phr_chunked_decoder {
        bytes_left_in_chunk: 0 as size_t,
        consume_trailer: 0,
        _hex_count: 0,
        _state: 0,
        _total_read: 0,
        _total_overhead: 0,
    };
    dec.consume_trailer = 1 as ::core::ffi::c_char;
    let mut buf: [::core::ffi::c_char; 34] =
        ::core::mem::transmute::<[u8; 34], [::core::ffi::c_char; 34]>(
            *b"5\r\nabcde\r\n0\r\n\r\nGET / HTTP/1.1\r\n\r\n\0",
        );
    let mut bufsz: size_t =
        (::core::mem::size_of::<[::core::ffi::c_char; 34]>() as size_t).wrapping_sub(1 as size_t);
    let mut ret: ssize_t = phr_decode_chunked(
        &raw mut dec,
        &raw mut buf as *mut ::core::ffi::c_char,
        &raw mut bufsz,
    );
    _ok(
        (ret >= 0 as ssize_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (bufsz == 5 as size_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (memcmp(
            &raw mut buf as *mut ::core::ffi::c_char as *const ::core::ffi::c_void,
            b"abcde\0" as *const u8 as *const ::core::ffi::c_char as *const ::core::ffi::c_void,
            5 as size_t,
        ) == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (ret as usize
            == (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as usize)
                .wrapping_sub(1 as usize)) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (memcmp(
            (&raw mut buf as *mut ::core::ffi::c_char).offset(bufsz as isize)
                as *const ::core::ffi::c_void,
            b"GET / HTTP/1.1\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char
                as *const ::core::ffi::c_void,
            (::core::mem::size_of::<[::core::ffi::c_char; 19]>() as size_t)
                .wrapping_sub(1 as size_t),
        ) == 0 as ::core::ffi::c_int) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
}
unsafe extern "C" fn do_test_chunked_overhead(
    mut chunk_len: size_t,
    mut chunk_count: size_t,
    mut extra: *const ::core::ffi::c_char,
) -> ssize_t {
    let mut current_block: u64;
    let mut dec: phr_chunked_decoder = phr_chunked_decoder {
        bytes_left_in_chunk: 0 as size_t,
        consume_trailer: 0,
        _hex_count: 0,
        _state: 0,
        _total_read: 0,
        _total_overhead: 0,
    };
    let mut buf: [::core::ffi::c_char; 1024] = [0; 1024];
    let mut bufsz: size_t = 0;
    let mut ret: ssize_t = 0;
    let mut i: size_t = 0 as size_t;
    loop {
        if !(i < chunk_count) {
            current_block = 2968425633554183086;
            break;
        }
        bufsz = sprintf(
            &raw mut buf as *mut ::core::ffi::c_char,
            b"%zx%s\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            chunk_len,
            extra,
        ) as size_t;
        ret = phr_decode_chunked(
            &raw mut dec,
            &raw mut buf as *mut ::core::ffi::c_char,
            &raw mut bufsz,
        );
        if ret != -(2 as ::core::ffi::c_int) as ssize_t {
            current_block = 13533104727637171266;
            break;
        }
        '_c2rust_label: {
            if bufsz == 0 as size_t {
            } else {
                __assert_fail(
                    b"bufsz == 0\0" as *const u8 as *const ::core::ffi::c_char,
                    b"/home/marche/noricum/tests/fixtures/picohttpparser/picohttpparser_combined.c\0"
                        as *const u8 as *const ::core::ffi::c_char,
                    1060 as ::core::ffi::c_uint,
                    b"ssize_t do_test_chunked_overhead(size_t, size_t, const char *)\0"
                        as *const u8 as *const ::core::ffi::c_char,
                );
            }
        };
        memset(
            &raw mut buf as *mut ::core::ffi::c_char as *mut ::core::ffi::c_void,
            'A' as i32,
            chunk_len,
        );
        bufsz = chunk_len;
        ret = phr_decode_chunked(
            &raw mut dec,
            &raw mut buf as *mut ::core::ffi::c_char,
            &raw mut bufsz,
        );
        if ret != -(2 as ::core::ffi::c_int) as ssize_t {
            current_block = 13533104727637171266;
            break;
        }
        '_c2rust_label_0: {
            if bufsz == chunk_len {
            } else {
                __assert_fail(
                    b"bufsz == chunk_len\0" as *const u8 as *const ::core::ffi::c_char,
                    b"/home/marche/noricum/tests/fixtures/picohttpparser/picohttpparser_combined.c\0"
                        as *const u8 as *const ::core::ffi::c_char,
                    1065 as ::core::ffi::c_uint,
                    b"ssize_t do_test_chunked_overhead(size_t, size_t, const char *)\0"
                        as *const u8 as *const ::core::ffi::c_char,
                );
            }
        };
        strcpy(
            &raw mut buf as *mut ::core::ffi::c_char,
            b"\r\n\0" as *const u8 as *const ::core::ffi::c_char,
        );
        bufsz = 2 as size_t;
        ret = phr_decode_chunked(
            &raw mut dec,
            &raw mut buf as *mut ::core::ffi::c_char,
            &raw mut bufsz,
        );
        if ret != -(2 as ::core::ffi::c_int) as ssize_t {
            current_block = 13533104727637171266;
            break;
        }
        '_c2rust_label_1: {
            if bufsz == 0 as size_t {
            } else {
                __assert_fail(
                    b"bufsz == 0\0" as *const u8 as *const ::core::ffi::c_char,
                    b"/home/marche/noricum/tests/fixtures/picohttpparser/picohttpparser_combined.c\0"
                        as *const u8 as *const ::core::ffi::c_char,
                    1070 as ::core::ffi::c_uint,
                    b"ssize_t do_test_chunked_overhead(size_t, size_t, const char *)\0"
                        as *const u8 as *const ::core::ffi::c_char,
                );
            }
        };
        i = i.wrapping_add(1);
    }
    match current_block {
        2968425633554183086 => {
            strcpy(
                &raw mut buf as *mut ::core::ffi::c_char,
                b"0\r\n\r\n\0" as *const u8 as *const ::core::ffi::c_char,
            );
            bufsz = 5 as size_t;
            ret = phr_decode_chunked(
                &raw mut dec,
                &raw mut buf as *mut ::core::ffi::c_char,
                &raw mut bufsz,
            );
            '_c2rust_label_2: {
                if bufsz == 0 as size_t {
                } else {
                    __assert_fail(
                        b"bufsz == 0\0" as *const u8 as *const ::core::ffi::c_char,
                        b"/home/marche/noricum/tests/fixtures/picohttpparser/picohttpparser_combined.c\0"
                            as *const u8 as *const ::core::ffi::c_char,
                        1076 as ::core::ffi::c_uint,
                        b"ssize_t do_test_chunked_overhead(size_t, size_t, const char *)\0"
                            as *const u8 as *const ::core::ffi::c_char,
                    );
                }
            };
        }
        _ => {}
    }
    return ret;
}
unsafe extern "C" fn test_chunked_overhead() {
    _ok(
        (do_test_chunked_overhead(
            100 as size_t,
            10000 as size_t,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ) == 2 as ssize_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (do_test_chunked_overhead(
            10 as size_t,
            100000 as ::core::ffi::c_int as size_t,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ) == 2 as ssize_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (do_test_chunked_overhead(
            1 as size_t,
            1000000 as ::core::ffi::c_int as size_t,
            b"\0" as *const u8 as *const ::core::ffi::c_char,
        ) == -(1 as ::core::ffi::c_int) as ssize_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (do_test_chunked_overhead(
            10 as size_t,
            100000 as ::core::ffi::c_int as size_t,
            b"; tiny=1\0" as *const u8 as *const ::core::ffi::c_char,
        ) == 2 as ssize_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
    _ok(
        (do_test_chunked_overhead(
            10 as size_t,
            100000 as ::core::ffi::c_int as size_t,
            b"; large=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0" as *const u8
                as *const ::core::ffi::c_char,
        ) == -(1 as ::core::ffi::c_int) as ssize_t) as ::core::ffi::c_int,
        b"check\0" as *const u8 as *const ::core::ffi::c_char,
    );
}
unsafe fn main_0() -> ::core::ffi::c_int {
    inputbuf = (&raw mut input_storage as *mut ::core::ffi::c_char).offset(INPUT_BUF_SIZE as isize);
    subtest(
        b"request\0" as *const u8 as *const ::core::ffi::c_char,
        Some(test_request as unsafe extern "C" fn() -> ()),
    );
    subtest(
        b"response\0" as *const u8 as *const ::core::ffi::c_char,
        Some(test_response as unsafe extern "C" fn() -> ()),
    );
    subtest(
        b"headers\0" as *const u8 as *const ::core::ffi::c_char,
        Some(test_headers as unsafe extern "C" fn() -> ()),
    );
    subtest(
        b"chunked\0" as *const u8 as *const ::core::ffi::c_char,
        Some(test_chunked as unsafe extern "C" fn() -> ()),
    );
    subtest(
        b"chunked-consume-trailer\0" as *const u8 as *const ::core::ffi::c_char,
        Some(test_chunked_consume_trailer as unsafe extern "C" fn() -> ()),
    );
    subtest(
        b"chunked-leftdata\0" as *const u8 as *const ::core::ffi::c_char,
        Some(test_chunked_leftdata as unsafe extern "C" fn() -> ()),
    );
    subtest(
        b"chunked-overhead\0" as *const u8 as *const ::core::ffi::c_char,
        Some(test_chunked_overhead as unsafe extern "C" fn() -> ()),
    );
    return done_testing();
}
pub fn main() {
    unsafe { ::std::process::exit(main_0() as i32) }
}
