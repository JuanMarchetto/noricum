//! lcorolib — the Lua `coroutine` library.
//!
//! Coroutines are implemented via the VM's Result-threading model:
//! `yield` stashes its values on the current thread's `pending_yield`
//! and raises `LuaError::Yield`, which bubbles up through `execute`
//! and `invoke_c_function`. `resume` catches the yield, switches
//! control back to the resumer thread, and on the next resume copies
//! the new values into the target slot so the yielding call site
//! sees them as the call's return values.
//!
//! This gives faithful coroutine semantics without an OS thread per
//! coroutine or a fully CPS-rewritten VM: the Rust call stack for
//! yielded Lua code is unwound through ordinary `?`, and re-entered
//! by simply calling `execute` again on the yielded thread.

#![allow(dead_code)]

use crate::contract::{
    LuaError, LuaState, RawCFunction, TValue, Table, TableHandle, Thread,
    ThreadHandle, ThreadStatus, ThreadStatusInner,
};

pub fn open_coroutine(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state.global.heap.alloc_table(Table::default());
    register(state, t, "create", co_create);
    register(state, t, "resume", co_resume);
    register(state, t, "yield", co_yield_fn);
    register(state, t, "status", co_status);
    register(state, t, "wrap", co_wrap);
    register(state, t, "isyieldable", co_isyieldable);
    register(state, t, "running", co_running);
    register(state, t, "close", co_close);

    let name = state.global.new_string(b"coroutine", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

/// `coroutine.create(f)` allocates a fresh thread, pushes `f` as the
/// first value on its stack, and returns the thread handle. Nothing
/// runs yet — execution starts on the first `resume`.
unsafe extern "C" fn co_create(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Read the function arg from the caller's frame.
    let f = match state.value_at_public(1) {
        Some(v @ (TValue::LuaClosure(_) | TValue::CClosure(_) | TValue::LightCFunction(_))) => v,
        _ => {
            let msg = state
                .global
                .new_string(b"bad argument to 'create' (function expected)", 0);
            state.raise_error_value(TValue::ShortString(msg));
            return 0;
        }
    };
    let mut th = Thread::default();
    th.status = ThreadStatusInner(ThreadStatus::Ok);
    th.stack = vec![TValue::Nil; 64];
    th.top = 1;
    th.stack[0] = f;
    th.is_coroutine = true;
    th.finished = false;
    let handle = state.global.heap.alloc_thread(th);
    state.current_thread_mut().push(TValue::Thread(handle));
    1
}

/// `coroutine.resume(co, ...)` — run the coroutine until it yields,
/// returns, or errors. Returns `(true, ...values)` on yield/return
/// and `(false, err)` on error.
unsafe extern "C" fn co_resume(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let co_handle = match state.value_at_public(1) {
        Some(TValue::Thread(h)) => h,
        _ => {
            state.push_boolean(false);
            state.push_string("attempt to resume non-coroutine");
            return 2;
        }
    };

    // Collect resume args (slots 2..top of the caller's frame).
    let top = state.get_top() as i32;
    let args: Vec<TValue> = (2..=top)
        .filter_map(|i| state.value_at_public(i))
        .collect();

    let caller = state.current_thread;
    let status = {
        let co = state.global.heap.thread(co_handle);
        co.status.0
    };

    let finished = state.global.heap.thread(co_handle).finished;
    if finished
        || matches!(
            status,
            ThreadStatus::RuntimeError
                | ThreadStatus::SyntaxError
                | ThreadStatus::MemoryError
                | ThreadStatus::GcError
                | ThreadStatus::HandlerError
        )
    {
        state.push_boolean(false);
        state.push_string("cannot resume dead coroutine");
        return 2;
    }

    // Record resumer so the coroutine's yield knows where to return.
    state.global.heap.thread_mut(co_handle).resumer = Some(caller);

    // Detect first-resume vs continued-resume by looking at frames.
    let is_first = state.global.heap.thread(co_handle).frames.is_empty();

    // Switch active thread.
    state.current_thread = co_handle;

    let outcome = if is_first {
        // First resume: push args after the function (already at slot 0).
        {
            let co = state.current_thread_mut();
            co.top = 1;
            for v in &args {
                co.push(*v);
            }
        }
        // call_value will drive execute() for Lua closures.
        state.call_value(0, args.len() as u32, -1)
    } else {
        // Continued resume: deliver args into the yield target slot,
        // pad/truncate to yield_n_expected, then re-enter execute().
        let target = state
            .current_thread()
            .yield_target_slot
            .expect("resume continuation without target slot");
        let n_expected = state
            .current_thread()
            .yield_n_expected
            .expect("resume continuation without expected count");
        let leave = if n_expected < 0 {
            args.len() as u32
        } else {
            n_expected as u32
        };
        {
            let co = state.current_thread_mut();
            // Grow if needed.
            let needed = (target + leave) as usize;
            if co.stack.len() < needed {
                co.stack.resize(needed, TValue::Nil);
            }
            for i in 0..leave {
                let v = args.get(i as usize).copied().unwrap_or(TValue::Nil);
                co.stack[(target + i) as usize] = v;
            }
            co.top = target + leave;
            co.yield_target_slot = None;
            co.yield_n_expected = None;
        }
        // Re-enter execute() in a loop. Each execute() invocation
        // returns when the topmost Lua frame's RETURN fires, but
        // outer Lua frames may still be active (e.g., a wrapper
        // function that called the yielding producer). Keep
        // resuming until either a Yield/error bubbles up or the
        // coroutine has no Lua frames left.
        let mut last = Ok(());
        while !state.current_thread().frames.is_empty() {
            last = state.execute();
            if last.is_err() {
                break;
            }
        }
        last
    };

    // Switch back to caller.
    state.current_thread = caller;

    match outcome {
        Ok(()) => {
            // Coroutine returned normally: copy values from co's top.
            let results: Vec<TValue> = {
                let co = state.global.heap.thread(co_handle);
                // After a normal return, co's remaining frames should
                // be empty (the function's frame was popped on RETURN).
                // Values live at stack[0..top].
                co.stack[..co.top as usize].to_vec()
            };
            {
                let th = state.global.heap.thread_mut(co_handle);
                th.status = ThreadStatusInner(ThreadStatus::Ok);
                th.finished = true;
            }
            state.push_boolean(true);
            for v in &results {
                state.current_thread_mut().push(*v);
            }
            (1 + results.len()) as i32
        }
        Err(LuaError::Yield) => {
            let vals = state
                .global
                .heap
                .thread_mut(co_handle)
                .pending_yield
                .take()
                .unwrap_or_default();
            state.global.heap.thread_mut(co_handle).status =
                ThreadStatusInner(ThreadStatus::Yield);
            state.push_boolean(true);
            for v in &vals {
                state.current_thread_mut().push(*v);
            }
            (1 + vals.len()) as i32
        }
        Err(err) => {
            {
                let th = state.global.heap.thread_mut(co_handle);
                th.status = ThreadStatusInner(err.matching_status());
                th.finished = true;
            }
            state.push_boolean(false);
            let errval = match err {
                LuaError::Runtime(v) => v,
                _ => TValue::Nil,
            };
            state.current_thread_mut().push(errval);
            2
        }
    }
}

/// `coroutine.yield(...)` — callable only from inside a coroutine.
unsafe extern "C" fn co_yield_fn(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if !state.current_thread().is_coroutine {
        let h = state.global.new_string(b"attempt to yield from outside a coroutine", 0);
        state.raise_error_value(TValue::ShortString(h));
        return 0;
    }
    let top = state.get_top() as i32;
    let vals: Vec<TValue> = (1..=top)
        .filter_map(|i| state.value_at_public(i))
        .collect();
    state.yield_values(vals);
    0
}

unsafe extern "C" fn co_status(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let handle = match state.value_at_public(1) {
        Some(TValue::Thread(h)) => h,
        _ => {
            state.push_string("dead");
            return 1;
        }
    };
    let cur = state.current_thread;
    let th = state.global.heap.thread(handle);
    let finished = th.finished;
    let s = th.status.0;
    let frames_empty = th.frames.is_empty();
    let top = th.top;
    let name = if handle == cur {
        "running"
    } else if finished {
        "dead"
    } else {
        match s {
            ThreadStatus::Ok => {
                // Ok with no frames AND non-empty stack = fresh (initial).
                // Ok with frames means suspended after a yield.
                if frames_empty && top == 0 {
                    "dead"
                } else {
                    "suspended"
                }
            }
            ThreadStatus::Yield => "suspended",
            _ => "dead",
        }
    };
    state.push_string(name);
    1
}

/// `coroutine.wrap(f)` — sugar for `local co = create(f); return function(...)
/// local ok, r = resume(co, ...); if not ok then error(r) end; return r`.
unsafe extern "C" fn co_wrap(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let f = match state.value_at_public(1) {
        Some(v @ (TValue::LuaClosure(_) | TValue::CClosure(_) | TValue::LightCFunction(_))) => v,
        _ => {
            let h = state.global.new_string(b"bad argument to 'wrap' (function expected)", 0);
            state.raise_error_value(TValue::ShortString(h));
            return 0;
        }
    };
    let mut th = Thread::default();
    th.status = ThreadStatusInner(ThreadStatus::Ok);
    th.stack = vec![TValue::Nil; 64];
    th.top = 1;
    th.stack[0] = f;
    th.is_coroutine = true;
    th.finished = false;
    let handle = state.global.heap.alloc_thread(th);

    // Build a C closure that captures the thread handle. Use Integer
    // slot + Thread slot as upvalues so the wrap callback can find
    // the coroutine on invocation.
    let cch = state.global.heap.alloc_cclosure(crate::contract::CClosure {
        f: wrap_call,
        upvalues: vec![TValue::Thread(handle)],
    });
    state.current_thread_mut().push(TValue::CClosure(cch));
    1
}

unsafe extern "C" fn wrap_call(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Fetch the coroutine handle from this CClosure's upvalue slot.
    let co = {
        let thread = state.current_thread();
        let frame = thread.frames.last().expect("wrap_call: no frame");
        let func_val = thread.stack[frame.func as usize];
        match func_val {
            TValue::CClosure(h) => match state.global.heap.cclosure(h).upvalues.first() {
                Some(TValue::Thread(t)) => *t,
                _ => return 0,
            },
            _ => return 0,
        }
    };
    let base = state.frame_base_index().unwrap_or(0);
    // Collect the user-passed args from the current frame.
    let top_arg = state.get_top() as i32;
    let args: Vec<TValue> = (1..=top_arg)
        .filter_map(|i| state.value_at_public(i))
        .collect();
    // Rewrite the frame to hold `co` + args so co_resume reads them.
    {
        let thread = state.current_thread_mut();
        thread.top = base;
        thread.push(TValue::Thread(co));
        for v in &args {
            thread.push(*v);
        }
    }
    let top_before_resume = state.current_thread().top;
    let n = unsafe { co_resume(state as *mut _) };
    // co_resume pushed its return values on top of our existing
    // frame contents. Their absolute slots are
    //   [top_before_resume .. top_before_resume + n).
    // The first is the ok-boolean; remaining are yielded/returned
    // values or an error.
    let result_base = top_before_resume as usize;
    let ok = matches!(
        state
            .current_thread()
            .stack
            .get(result_base)
            .copied(),
        Some(TValue::True)
    );
    if !ok {
        let err = state
            .current_thread()
            .stack
            .get(result_base + 1)
            .copied()
            .unwrap_or(TValue::Nil);
        state.raise_error_value(err);
        return 0;
    }
    // Keep values at slots [result_base+1 .. result_base+n) and
    // hand them back as wrap's return values. We do that by
    // shifting them down into [base .. base + (n-1)) and trimming
    // top so finish_c_call copies exactly (n-1) values to the
    // caller's slot.
    let keep = (n - 1).max(0) as u32;
    {
        let thread = state.current_thread_mut();
        for i in 0..keep {
            thread.stack[base as usize + i as usize] =
                thread.stack[result_base + 1 + i as usize];
        }
        thread.top = base + keep;
    }
    keep as i32
}

unsafe extern "C" fn co_isyieldable(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_boolean(state.current_thread().is_coroutine);
    1
}

unsafe extern "C" fn co_running(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let handle = state.current_thread;
    let is_main = !state.current_thread().is_coroutine;
    state.current_thread_mut().push(TValue::Thread(handle));
    state.push_boolean(is_main);
    2
}

unsafe extern "C" fn co_close(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let handle = match state.value_at_public(1) {
        Some(TValue::Thread(h)) => h,
        _ => {
            state.push_boolean(false);
            return 1;
        }
    };
    {
        let th = state.global.heap.thread_mut(handle);
        th.status = ThreadStatusInner(ThreadStatus::Ok);
        th.frames.clear();
        th.open_upvals.clear();
        th.top = 0;
        th.pending_yield = None;
        th.yield_target_slot = None;
        th.yield_n_expected = None;
    }
    state.push_boolean(true);
    1
}
