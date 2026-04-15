//! liolib — the Lua `io` library with real file handles.
//!
//! Open files live in a process-wide registry keyed by u64 id;
//! the id is serialized into the 8-byte payload of a `UserData`
//! so the GC can reach the Lua-visible handle. Closing (explicit
//! or via process exit) removes the registry entry.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle, UserData};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

enum FileEntry {
    Read(BufReader<File>),
    Write(File),
    ReadWrite(File),
    Stdout,
    Stderr,
    Stdin,
}

fn registry() -> &'static Mutex<HashMap<u64, FileEntry>> {
    static REG: OnceLock<Mutex<HashMap<u64, FileEntry>>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(1, FileEntry::Stdin);
        m.insert(2, FileEntry::Stdout);
        m.insert(3, FileEntry::Stderr);
        Mutex::new(m)
    })
}

fn next_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(100);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Default-output / default-input ids used by `io.write` /
/// `io.read` when called without an explicit file handle. Set by
/// `io.output(...)` / `io.input(...)`. Initial values point at
/// stdout (2) and stdin (1) — the classic Unix defaults.
static DEFAULT_OUTPUT_ID: AtomicU64 = AtomicU64::new(2);
static DEFAULT_INPUT_ID: AtomicU64 = AtomicU64::new(1);

fn encode_id(id: u64) -> Vec<u8> {
    id.to_le_bytes().to_vec()
}

fn decode_id(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        return None;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(&data[..8]);
    Some(u64::from_le_bytes(b))
}

fn userdata_id(state: &mut LuaState, idx: i32) -> Option<u64> {
    let base = state.frame_base_index().unwrap_or(0) as i32;
    let slot = (base + idx - 1).max(0) as usize;
    let v = state.current_thread().stack.get(slot).copied()?;
    if let TValue::UserData(h) = v {
        decode_id(&state.global.heap.userdata_get(h).data)
    } else {
        None
    }
}

pub fn open_io(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, t, "read", io_read);
    register(state, t, "write", io_write);
    register(state, t, "open", io_open);
    register(state, t, "close", io_close);
    register(state, t, "lines", io_lines);
    register(state, t, "flush", io_flush);
    register(state, t, "output", io_output);
    register(state, t, "input", io_input);
    register(state, t, "type", io_type);

    let file_mt = install_file_metatable(state);
    set_file_mt(file_mt);
    set_std_stream(state, t, "stdout", 2, file_mt);
    set_std_stream(state, t, "stderr", 3, file_mt);
    set_std_stream(state, t, "stdin", 1, file_mt);

    let name = state.global.new_string(b"io", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

/// Cache the file-handle metatable so new `io.open` results can
/// install it without re-traversing the globals table.
fn set_file_mt(mt: TableHandle) {
    FILE_MT.set(FileMt { slot: mt.slot, generation: mt.generation }).ok();
}
fn get_file_mt() -> Option<TableHandle> {
    FILE_MT
        .get()
        .map(|m| TableHandle::new(m.slot, m.generation))
}
#[derive(Clone, Copy)]
struct FileMt { slot: u32, generation: u32 }
static FILE_MT: OnceLock<FileMt> = OnceLock::new();

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

fn install_file_metatable(state: &mut LuaState) -> TableHandle {
    let methods = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, methods, "read", file_read);
    register(state, methods, "write", file_write);
    register(state, methods, "close", file_close);
    register(state, methods, "lines", file_lines);
    register(state, methods, "seek", file_seek);
    register(state, methods, "flush", file_flush);
    register(state, methods, "setvbuf", file_setvbuf);

    let mt = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    let idx_k = state.global.new_string(b"__index", 0);
    state
        .global
        .table_set_shortstr(mt, idx_k, TValue::Table(methods));
    mt
}

fn set_std_stream(
    state: &mut LuaState,
    io_t: TableHandle,
    name: &str,
    id: u64,
    file_mt: TableHandle,
) {
    let ud = state.global.heap.alloc_userdata(UserData {
        data: encode_id(id),
        metatable: Some(file_mt),
        user_values: vec![],
    });
    let key = state.global.new_string(name.as_bytes(), 0);
    state
        .global
        .table_set_shortstr(io_t, key, TValue::UserData(ud));
}

fn make_file_userdata(state: &mut LuaState, id: u64) -> TValue {
    let mt = get_file_mt();
    let ud = state.global.heap.alloc_userdata(UserData {
        data: encode_id(id),
        metatable: mt,
        user_values: vec![],
    });
    TValue::UserData(ud)
}

// ---------- io.* free functions ----------

unsafe extern "C" fn io_read(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    read_from_id(state, 1, 1)
}

unsafe extern "C" fn io_write(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    let mut buf = Vec::new();
    for i in 1..=top {
        append_tv_as_bytes(state, i, &mut buf);
    }
    let id = DEFAULT_OUTPUT_ID.load(Ordering::Relaxed);
    write_to_id(id, &buf);
    0
}

/// Write `buf` to the file-handle identified by `id` in the
/// process-wide registry. Handles stdout/stderr inline; for opened
/// files locates the entry, writes, and re-inserts.
fn write_to_id(id: u64, buf: &[u8]) {
    if id == 2 {
        let _ = std::io::stdout().write_all(buf);
        return;
    }
    if id == 3 {
        let _ = std::io::stderr().write_all(buf);
        return;
    }
    let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = reg.remove(&id) {
        let new_entry = match entry {
            FileEntry::Write(mut f) => {
                let _ = f.write_all(buf);
                FileEntry::Write(f)
            }
            FileEntry::ReadWrite(mut f) => {
                let _ = f.write_all(buf);
                FileEntry::ReadWrite(f)
            }
            other => other,
        };
        reg.insert(id, new_entry);
    }
}

unsafe extern "C" fn io_open(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let path = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let mode_bytes = state
        .to_lstring(2)
        .map(|s| s.to_vec())
        .unwrap_or_else(|| b"r".to_vec());
    let p = String::from_utf8_lossy(&path).to_string();
    let m = String::from_utf8_lossy(&mode_bytes).to_string();

    let (read, write, append, truncate, create) = classify_mode(&m);
    let mut opts = OpenOptions::new();
    opts.read(read)
        .write(write)
        .append(append)
        .truncate(truncate)
        .create(create);

    let file = match opts.open(&p) {
        Ok(f) => f,
        Err(e) => {
            state.push_nil();
            state.push_string(&format!("{}: {}", p, e));
            return 2;
        }
    };

    let entry = if read && !write && !append {
        FileEntry::Read(BufReader::new(file))
    } else if (write || append) && !read {
        FileEntry::Write(file)
    } else {
        FileEntry::ReadWrite(file)
    };
    let id = next_id();
    registry().lock().unwrap().insert(id, entry);
    let v = make_file_userdata(state, id);
    state.current_thread_mut().push(v);
    1
}

unsafe extern "C" fn io_close(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let id = userdata_id(state, 1).unwrap_or(0);
    close_id(id);
    state.push_boolean(true);
    1
}

unsafe extern "C" fn io_lines(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let path = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let p = String::from_utf8_lossy(&path).to_string();
    let file = match File::open(&p) {
        Ok(f) => f,
        Err(e) => {
            state.push_nil();
            state.push_string(&format!("{}: {}", p, e));
            return 2;
        }
    };
    let id = next_id();
    registry()
        .lock()
        .unwrap()
        .insert(id, FileEntry::Read(BufReader::new(file)));
    push_lines_iterator(state, id, true);
    1
}

unsafe extern "C" fn io_flush(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let _ = std::io::stdout().flush();
    state.push_boolean(true);
    1
}

/// `io.output([file])` — set or read the default output. Accepts
/// either a filename (opens it for writing and uses that) or a file
/// handle userdata (uses it directly). With no argument, returns the
/// current default-output handle.
unsafe extern "C" fn io_output(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    set_default_stream(state, /*is_output=*/ true)
}

/// `io.input([file])` — same shape as `io.output`, but for the
/// default input.
unsafe extern "C" fn io_input(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    set_default_stream(state, /*is_output=*/ false)
}

fn set_default_stream(state: &mut LuaState, is_output: bool) -> std::os::raw::c_int {
    let top = state.get_top();
    let cell = if is_output { &DEFAULT_OUTPUT_ID } else { &DEFAULT_INPUT_ID };
    if top == 0 {
        // Read current default — push a fresh userdata that wraps
        // the existing id (good enough for callers that only check
        // truthiness or pass it around).
        let id = cell.load(Ordering::Relaxed);
        push_file_handle(state, id);
        return 1;
    }
    if let Some(path_bytes) = state.to_lstring(1).map(|s| s.to_vec()) {
        let p = String::from_utf8_lossy(&path_bytes).to_string();
        let mode = if is_output { "w" } else { "r" };
        let (read, write, append, truncate, create) = classify_mode(mode);
        let mut opts = OpenOptions::new();
        opts.read(read)
            .write(write)
            .append(append)
            .truncate(truncate)
            .create(create);
        let file = match opts.open(&p) {
            Ok(f) => f,
            Err(e) => {
                let msg = format!("{}: {}", p, e);
                let h = state.global.new_string(msg.as_bytes(), 0);
                state.raise_error_value(crate::contract::TValue::ShortString(h));
                return 0;
            }
        };
        let id = next_id();
        let entry = if is_output {
            FileEntry::Write(file)
        } else {
            FileEntry::Read(BufReader::new(file))
        };
        registry().lock().unwrap_or_else(|e| e.into_inner()).insert(id, entry);
        cell.store(id, Ordering::Relaxed);
        push_file_handle(state, id);
        return 1;
    }
    // Userdata case: extract the id and store it as the default.
    if let Some(id) = userdata_id(state, 1) {
        cell.store(id, Ordering::Relaxed);
    }
    state.push_value(1);
    1
}

/// Wrap an existing file id in a fresh userdata + file metatable so
/// the caller can use it as a Lua file handle.
fn push_file_handle(state: &mut LuaState, id: u64) {
    let v = make_file_userdata(state, id);
    state.current_thread_mut().push(v);
}

/// `io.type(x)` — returns "file" if x is a file handle (open), or
/// "closed file" if closed, or nil otherwise. Cheap shim — we don't
/// distinguish open vs closed here, so report "file" for any handle
/// userdata.
unsafe extern "C" fn io_type(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if userdata_id(state, 1).is_some() {
        state.push_string("file");
    } else {
        state.push_nil();
    }
    1
}

// ---------- file:* methods ----------

unsafe extern "C" fn file_read(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let Some(id) = userdata_id(state, 1) else {
        state.push_nil();
        return 1;
    };
    read_from_id(state, 2, id)
}

unsafe extern "C" fn file_write(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let Some(id) = userdata_id(state, 1) else {
        state.push_nil();
        return 1;
    };
    let top = state.get_top() as i32;
    let mut buf = Vec::new();
    for i in 2..=top {
        append_tv_as_bytes(state, i, &mut buf);
    }
    let mut reg = registry().lock().unwrap();
    let ok = match reg.get_mut(&id) {
        Some(FileEntry::Write(f)) | Some(FileEntry::ReadWrite(f)) => f.write_all(&buf).is_ok(),
        Some(FileEntry::Stdout) => std::io::stdout().write_all(&buf).is_ok(),
        Some(FileEntry::Stderr) => std::io::stderr().write_all(&buf).is_ok(),
        _ => false,
    };
    drop(reg);
    if ok {
        let base = state.frame_base_index().unwrap_or(0) as usize;
        let self_v = state
            .current_thread()
            .stack
            .get(base)
            .copied()
            .unwrap_or(TValue::Nil);
        state.current_thread_mut().push(self_v);
        1
    } else {
        state.push_nil();
        state.push_string("write failed");
        2
    }
}

unsafe extern "C" fn file_close(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let id = userdata_id(state, 1).unwrap_or(0);
    close_id(id);
    state.push_boolean(true);
    1
}

unsafe extern "C" fn file_lines(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let id = userdata_id(state, 1).unwrap_or(0);
    push_lines_iterator(state, id, false);
    1
}

unsafe extern "C" fn file_seek(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let id = userdata_id(state, 1).unwrap_or(0);
    let whence = state
        .to_lstring(2)
        .map(|s| s.to_vec())
        .unwrap_or_else(|| b"cur".to_vec());
    let offset = state.to_integer_x(3).unwrap_or(0);
    let pos = match whence.as_slice() {
        b"set" => SeekFrom::Start(offset.max(0) as u64),
        b"end" => SeekFrom::End(offset),
        _ => SeekFrom::Current(offset),
    };
    let mut reg = registry().lock().unwrap();
    let result = match reg.get_mut(&id) {
        Some(FileEntry::Write(f)) | Some(FileEntry::ReadWrite(f)) => f.seek(pos),
        Some(FileEntry::Read(br)) => br.seek(pos),
        _ => {
            drop(reg);
            state.push_nil();
            state.push_string("seek on invalid file");
            return 2;
        }
    };
    drop(reg);
    match result {
        Ok(p) => {
            state.push_integer(p as i64);
            1
        }
        Err(e) => {
            state.push_nil();
            state.push_string(&format!("{}", e));
            2
        }
    }
}

unsafe extern "C" fn file_flush(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let id = userdata_id(state, 1).unwrap_or(0);
    let mut reg = registry().lock().unwrap();
    match reg.get_mut(&id) {
        Some(FileEntry::Write(f)) | Some(FileEntry::ReadWrite(f)) => {
            let _ = f.flush();
        }
        Some(FileEntry::Stdout) => {
            let _ = std::io::stdout().flush();
        }
        Some(FileEntry::Stderr) => {
            let _ = std::io::stderr().flush();
        }
        _ => {}
    }
    state.push_boolean(true);
    1
}

unsafe extern "C" fn file_setvbuf(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_boolean(true);
    1
}

// ---------- line iterators ----------

/// `lines` iterators carry their file id + "close-on-eof" flag via
/// a CClosure with two Integer upvalues. Allocation + push is the
/// C-closure-with-upvalues primitive expressed directly against
/// the heap.
fn push_lines_iterator(state: &mut LuaState, id: u64, close_on_eof: bool) {
    let cch = state
        .global
        .heap
        .alloc_cclosure(crate::contract::CClosure {
            f: lines_iter,
            upvalues: vec![
                TValue::Integer(id as i64),
                TValue::Integer(if close_on_eof { 1 } else { 0 }),
            ],
        });
    state.current_thread_mut().push(TValue::CClosure(cch));
}

unsafe extern "C" fn lines_iter(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Read upvalues off the currently-invoked CClosure. We reach
    // them via the current frame's function slot + the cclosure
    // table on the heap.
    let (id, close_on_eof) = {
        let thread = state.current_thread();
        let frame = thread.frames.last().expect("lines_iter: no frame");
        let func_val = thread.stack[frame.func as usize];
        match func_val {
            TValue::CClosure(h) => {
                let cc = state.global.heap.cclosure(h);
                let id = match cc.upvalues.first() {
                    Some(TValue::Integer(i)) => *i as u64,
                    _ => 0,
                };
                let c = matches!(cc.upvalues.get(1), Some(TValue::Integer(1)));
                (id, c)
            }
            _ => (0, false),
        }
    };
    if let Some(line) = read_one_line(id) {
        let h = state.push_lstring(&line);
        let _ = h;
    } else {
        if close_on_eof {
            close_id(id);
        }
        state.push_nil();
    }
    1
}

fn read_one_line(id: u64) -> Option<Vec<u8>> {
    let mut reg = registry().lock().unwrap();
    let entry = reg.get_mut(&id)?;
    let mut line = Vec::new();
    let n = match entry {
        FileEntry::Read(br) => br.read_until(b'\n', &mut line).ok()?,
        FileEntry::ReadWrite(f) => {
            let mut br = BufReader::new(&mut *f);
            br.read_until(b'\n', &mut line).ok()?
        }
        FileEntry::Stdin => std::io::stdin().lock().read_until(b'\n', &mut line).ok()?,
        _ => return None,
    };
    if n == 0 {
        return None;
    }
    if line.ends_with(b"\n") {
        line.pop();
    }
    if line.ends_with(b"\r") {
        line.pop();
    }
    Some(line)
}

fn close_id(id: u64) {
    if id <= 3 {
        return;
    }
    let mut reg = registry().lock().unwrap();
    reg.remove(&id);
}

fn read_from_id(state: &mut LuaState, first_arg: i32, id: u64) -> std::os::raw::c_int {
    let top = state.get_top() as i32;
    if top < first_arg {
        return read_format(state, id, b"*l");
    }
    let mut returned = 0;
    for i in first_arg..=top {
        let fmt = state.to_lstring(i).map(|s| s.to_vec()).unwrap_or_default();
        returned += read_format(state, id, &fmt);
    }
    returned
}

fn read_format(state: &mut LuaState, id: u64, fmt: &[u8]) -> std::os::raw::c_int {
    match fmt {
        b"*l" | b"l" | b"*L" | b"L" => {
            if let Some(line) = read_one_line(id) {
                let _ = state.push_lstring(&line);
            } else {
                state.push_nil();
            }
            1
        }
        b"*a" | b"a" => {
            let mut buf = Vec::new();
            let mut reg = registry().lock().unwrap();
            match reg.get_mut(&id) {
                Some(FileEntry::Read(br)) => {
                    let _ = br.read_to_end(&mut buf);
                }
                Some(FileEntry::ReadWrite(f)) => {
                    let _ = f.read_to_end(&mut buf);
                }
                Some(FileEntry::Stdin) => {
                    let _ = std::io::stdin().read_to_end(&mut buf);
                }
                _ => {}
            }
            drop(reg);
            let _ = state.push_lstring(&buf);
            1
        }
        b"*n" | b"n" => {
            if let Some(line) = read_one_line(id) {
                if let Ok(s) = std::str::from_utf8(&line) {
                    if let Ok(i) = s.trim().parse::<i64>() {
                        state.push_integer(i);
                        return 1;
                    }
                    if let Ok(f) = s.trim().parse::<f64>() {
                        state.push_number(f);
                        return 1;
                    }
                }
            }
            state.push_nil();
            1
        }
        _ => {
            // Treat unknown format as byte count if it's numeric.
            if let Ok(s) = std::str::from_utf8(fmt) {
                if let Ok(n) = s.trim().parse::<usize>() {
                    let mut buf = vec![0u8; n];
                    let mut reg = registry().lock().unwrap();
                    let read_n = match reg.get_mut(&id) {
                        Some(FileEntry::Read(br)) => br.read(&mut buf).unwrap_or(0),
                        Some(FileEntry::ReadWrite(f)) => f.read(&mut buf).unwrap_or(0),
                        Some(FileEntry::Stdin) => std::io::stdin().read(&mut buf).unwrap_or(0),
                        _ => 0,
                    };
                    drop(reg);
                    if read_n == 0 {
                        state.push_nil();
                    } else {
                        buf.truncate(read_n);
                        let _ = state.push_lstring(&buf);
                    }
                    return 1;
                }
            }
            // Default to one line.
            if let Some(line) = read_one_line(id) {
                let _ = state.push_lstring(&line);
            } else {
                state.push_nil();
            }
            1
        }
    }
}

fn append_tv_as_bytes(state: &mut LuaState, idx: i32, out: &mut Vec<u8>) {
    if let Some(s) = state.to_lstring(idx) {
        out.extend_from_slice(s);
        return;
    }
    if let Some(i) = state.to_integer_x(idx) {
        out.extend_from_slice(i.to_string().as_bytes());
        return;
    }
    if let Some(f) = state.to_number_x(idx) {
        out.extend_from_slice(crate::lbaselib::format_lua_number(f).as_bytes());
    }
}

fn classify_mode(m: &str) -> (bool, bool, bool, bool, bool) {
    let mut read = false;
    let mut write = false;
    let mut append = false;
    let plus = m.contains('+');
    if m.starts_with('r') {
        read = true;
        if plus {
            write = true;
        }
    } else if m.starts_with('w') {
        write = true;
        if plus {
            read = true;
        }
    } else if m.starts_with('a') {
        append = true;
        write = true;
        if plus {
            read = true;
        }
    } else {
        read = true;
    }
    let truncate = m.starts_with('w');
    let create = m.starts_with('w') || m.starts_with('a');
    (read, write, append, truncate, create)
}
