//! lparser — the Lua parser.
//!
//! Port of Lua's `lparser.c`. Recursive descent parser that emits
//! bytecode on-the-fly through `lcode.rs` — no AST intermediate.
//! This module defines the grammar types and the parser proper.

#![allow(dead_code)]

use crate::contract::{
    LClosure, LuaInteger, LuaNumber, LuaState, Proto, StringHandle, TValue, UpvalDesc,
};
use crate::lcode::{self, BinOpr, UnOpr, NO_JUMP};
use crate::llex::{
    LexState, SemInfo, TK_AND, TK_BREAK, TK_CONCAT, TK_DBCOLON, TK_DO, TK_DOTS,
    TK_ELSE, TK_ELSEIF, TK_END, TK_EOS, TK_EQ, TK_FALSE, TK_FOR, TK_FUNCTION,
    TK_GE, TK_GOTO, TK_IDIV, TK_IF, TK_IN, TK_INT, TK_LE, TK_LOCAL, TK_NAME,
    TK_NE, TK_NIL, TK_NOT, TK_OR, TK_REPEAT, TK_RETURN, TK_SHL, TK_SHR,
    TK_STRING, TK_THEN, TK_TRUE, TK_UNTIL, TK_WHILE, TK_FLT,
};

/// Maximum number of locals per function.
pub const MAXVARS: usize = 200;

// ---- Expression descriptor -------------------------------------------------

/// Kind of an expression node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpKind {
    Void,
    Nil,
    True,
    False,
    /// Constant in k; info = index.
    K,
    /// Float constant; nval.
    KFlt,
    /// Integer constant; ival.
    KInt,
    /// String constant; strval.
    KStr,
    /// Value in fixed register; info = register.
    NonReloc,
    /// Local variable; var.ridx + var.vidx.
    Local,
    /// Upvalue; info = upvalue index.
    Upval,
    /// Indexed variable (t[key]); uses ind field.
    Indexed,
    /// Indexed upvalue; ind.
    IndexUp,
    /// Indexed with integer constant; ind.
    IndexI,
    /// Indexed with string constant; ind.
    IndexStr,
    /// Test/comparison jump; info = pc.
    Jmp,
    /// Relocatable; info = instruction pc.
    Reloc,
    /// Function call; info = instruction pc.
    Call,
    /// Vararg expression; info = instruction pc.
    VarArg,
}

/// An indexed-access descriptor.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndDesc {
    pub idx: i16,
    pub t: u8,
    pub key_k: i32,
}

/// A local-variable descriptor within an expression.
#[derive(Debug, Clone, Copy, Default)]
pub struct VarDesc {
    pub ridx: u8,
    pub vidx: i16,
}

/// Expression descriptor — central type flowing through the parser.
#[derive(Debug, Clone)]
pub struct ExprDesc {
    pub k: ExpKind,
    pub ival: LuaInteger,
    pub nval: LuaNumber,
    pub strval: Option<StringHandle>,
    pub info: i32,
    pub ind: IndDesc,
    pub var: VarDesc,
    /// Patch list: exit when true.
    pub t: i32,
    /// Patch list: exit when false.
    pub f: i32,
}

impl ExprDesc {
    pub fn void() -> Self {
        ExprDesc {
            k: ExpKind::Void,
            ival: 0,
            nval: 0.0,
            strval: None,
            info: 0,
            ind: IndDesc::default(),
            var: VarDesc::default(),
            t: NO_JUMP,
            f: NO_JUMP,
        }
    }

    pub fn init(k: ExpKind, info: i32) -> Self {
        let mut e = Self::void();
        e.k = k;
        e.info = info;
        e
    }
}

// ---- Active variable descriptor --------------------------------------------

/// Tracks a local variable's declaration during compilation.
#[derive(Debug, Clone)]
pub struct ActiveVar {
    pub name: Option<StringHandle>,
    pub kind: u8,
    pub ridx: u8,
    pub pidx: i16,
}

pub const VDKREG: u8 = 0;
pub const VDKCONST: u8 = 1;
pub const VDKTOCLOSE: u8 = 2;
pub const VDKCTC: u8 = 3;

// ---- Block scope -----------------------------------------------------------

#[derive(Debug)]
pub struct BlockCnt {
    pub firstlabel: usize,
    pub firstgoto: usize,
    pub nactvar: i16,
    pub upval: bool,
    pub is_loop: u8,
    pub inside_tbc: bool,
    /// Pending `break` jump PCs collected within this block.
    /// The ends of enclosing loops patch these to the post-loop
    /// PC.
    pub breaks: Vec<i32>,
}

// ---- Label / Goto ----------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LabelDesc {
    pub name: Option<StringHandle>,
    pub pc: i32,
    pub line: i32,
    pub nactvar: i16,
    pub close: bool,
}

// ---- Function state --------------------------------------------------------

/// Per-function compilation state. Holds the Proto being built and
/// all bookkeeping for registers, locals, labels, and gotos.
pub struct FuncState {
    pub proto: Proto,
    pub pc: i32,
    pub lasttarget: i32,
    pub previousline: i32,
    pub nk: i32,
    pub np: i32,
    pub first_local: usize,
    pub first_label: usize,
    pub n_debug_vars: i16,
    pub nactvar: i16,
    pub nups: u8,
    pub freereg: u8,
    pub need_close: bool,
    /// Active variable descriptors for this function's scope.
    pub actvar: Vec<ActiveVar>,
    /// Pending gotos.
    pub gotos: Vec<LabelDesc>,
    /// Active labels.
    pub labels: Vec<LabelDesc>,
    /// Block scope stack.
    pub blocks: Vec<BlockCnt>,
    /// Snapshot of outer function's locals (for upvalue capture).
    /// Empty for the outermost chunk.
    pub outer_locals: Vec<ActiveVar>,
    /// Active var count from the outer function.
    pub outer_nactvar: i16,
}

impl FuncState {
    pub fn new() -> Self {
        FuncState {
            proto: Proto::default(),
            pc: 0,
            lasttarget: 0,
            previousline: 0,
            nk: 0,
            np: 0,
            first_local: 0,
            first_label: 0,
            n_debug_vars: 0,
            nactvar: 0,
            nups: 0,
            freereg: 0,
            need_close: false,
            actvar: Vec::new(),
            gotos: Vec::new(),
            labels: Vec::new(),
            blocks: Vec::new(),
            outer_locals: Vec::new(),
            outer_nactvar: 0,
        }
    }
}

// ---- Parser entry ----------------------------------------------------------

/// Compile a Lua source string into an LClosure ready for
/// execution. The closure's single upvalue (_ENV) is bound to
/// `globals` if provided, else to a fresh empty table.
pub fn parse_with_env(
    state: &mut LuaState,
    source: &[u8],
    source_name: &[u8],
    globals: crate::contract::TableHandle,
) -> crate::contract::LClosureHandle {
    let closure = parse(state, source, source_name);
    // Bind the _ENV upvalue (index 0) to `globals`.
    let env_uv = state.global.heap.alloc_upval(crate::contract::UpVal {
        state: crate::contract::UpValState::Closed(TValue::Table(globals)),
    });
    // Replace the closure's upvalue list.
    let lc = state.global.heap.lclosure_mut(closure);
    lc.upvalues = vec![env_uv];
    closure
}

/// Compile a Lua source string into an LClosure ready for
/// execution. Returns the allocated closure handle.
pub fn parse(
    state: &mut LuaState,
    source: &[u8],
    source_name: &[u8],
) -> crate::contract::LClosureHandle {
    let name_handle = state.global.new_string(source_name, state.global.hash_seed);
    let mut ls = LexState::new(&mut state.global, source, name_handle);
    ls.next_token(); // prime the first token

    let mut fs = FuncState::new();
    fs.proto.is_vararg = true; // main chunk is always vararg
    fs.proto.max_stack_size = 2;

    // Open the main block.
    fs.blocks.push(BlockCnt {
        firstlabel: 0,
        firstgoto: 0,
        nactvar: 0,
        upval: false,
        is_loop: 0,
        inside_tbc: false,
        breaks: Vec::new(),
    });

    // Add _ENV upvalue (index 0).
    fs.proto.upvalues.push(UpvalDesc {
        name: None,
        in_stack: true,
        idx: 0,
        kind: 0,
    });
    fs.nups = 1;

    // Parse the body.
    statlist(&mut ls, &mut fs);

    // Check we consumed everything.
    if ls.t.token != TK_EOS {
        ls.syntax_error("<eof> expected");
    }

    // Emit final RETURN0.
    lcode::code_return(&mut fs, 0, 0);

    // Close the main block.
    fs.blocks.pop();

    // Finalize proto.
    fs.proto.max_stack_size = fs.proto.max_stack_size.max(fs.freereg);

    // Allocate on the heap.
    let gs = unsafe { &mut *ls.gs };
    let proto_handle = gs.heap.alloc_proto(fs.proto);
    let closure = LClosure {
        proto: proto_handle,
        upvalues: vec![],
    };
    gs.heap.alloc_lclosure(closure)
}

// ---- Grammar rules ---------------------------------------------------------

fn statlist(ls: &mut LexState, fs: &mut FuncState) {
    while !block_follow(ls, true) {
        if ls.t.token == TK_RETURN {
            statement(ls, fs);
            return;
        }
        statement(ls, fs);
    }
}

fn block_follow(ls: &LexState, with_until: bool) -> bool {
    match ls.t.token {
        TK_ELSE | TK_ELSEIF | TK_END | TK_EOS => true,
        TK_UNTIL => with_until,
        _ => false,
    }
}

fn statement(ls: &mut LexState, fs: &mut FuncState) {
    match ls.t.token {
        TK_IF => ifstat(ls, fs),
        TK_WHILE => whilestat(ls, fs),
        TK_DO => {
            ls.next_token(); // skip DO
            block(ls, fs);
            check_match(ls, TK_END, TK_DO);
        }
        TK_FOR => forstat(ls, fs),
        TK_REPEAT => repeatstat(ls, fs),
        TK_FUNCTION => funcstat(ls, fs),
        TK_LOCAL => localstat(ls, fs),
        TK_RETURN => retstat(ls, fs),
        TK_DBCOLON => labelstat(ls, fs),
        TK_GOTO => gotostat(ls, fs),
        TK_BREAK => {
            ls.next_token();
            // Find the nearest enclosing loop and record the JMP
            // PC in its break_chain for later patching.
            let jmp = lcode::emit_jump(fs);
            let mut patched = false;
            for bl in fs.blocks.iter_mut().rev() {
                if bl.is_loop != 0 {
                    bl.breaks.push(jmp);
                    patched = true;
                    break;
                }
            }
            if !patched {
                ls.syntax_error("break outside a loop");
            }
        }
        x if x == b';' as i32 => {
            ls.next_token(); // skip ';'
        }
        _ => exprstat(ls, fs),
    }
    // Reset freereg to nvarstack at the end of each statement —
    // matches C Lua's `leavelevel` / per-statement temporary
    // cleanup. Without this, intermediate temps from one
    // statement leak into the next, shifting register slots.
    let needed = nvarstack(fs);
    if fs.freereg > needed {
        fs.freereg = needed;
    }
}

/// `::labelname::` — define a label for `goto`.
fn labelstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip `::`
    let name = check_name(ls);
    check_next(ls, TK_DBCOLON);
    let pc = lcode::get_label(fs);
    let line = ls.linenumber;
    fs.labels.push(LabelDesc {
        name: Some(name),
        pc,
        line,
        nactvar: fs.nactvar,
        close: false,
    });
    // Resolve any pending gotos that target this label.
    let mut i = 0;
    while i < fs.gotos.len() {
        if fs.gotos[i].name == Some(name) {
            let g = fs.gotos.remove(i);
            lcode::patch_list(fs, g.pc, pc);
        } else {
            i += 1;
        }
    }
}

/// `goto labelname` — emit a JMP to be patched when the label
/// becomes known.
fn gotostat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip `goto`
    let name = check_name(ls);
    let line = ls.linenumber;
    // If a matching label is already defined, patch straight to
    // its pc. Otherwise, emit an unresolved JMP and park it on
    // fs.gotos for the next `::label::` to pick up.
    let existing = fs
        .labels
        .iter()
        .rfind(|l| l.name == Some(name))
        .map(|l| l.pc);
    let jmp = lcode::emit_jump(fs);
    match existing {
        Some(target) => {
            lcode::patch_list(fs, jmp, target);
        }
        None => {
            fs.gotos.push(LabelDesc {
                name: Some(name),
                pc: jmp,
                line,
                nactvar: fs.nactvar,
                close: false,
            });
        }
    }
}

fn retstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip RETURN
    let first = fs.freereg;
    let nret;
    if block_follow(ls, true) || ls.t.token == b';' as i32 {
        nret = 0;
    } else {
        nret = explist(ls, fs);
    }
    // After explist, results occupy consecutive registers ending
    // at fs.freereg - 1. The first return register is
    // fs.freereg - nret.
    let actual_first = (fs.freereg as i32) - nret;
    lcode::code_return(fs, actual_first, nret);
    let _ = first;
    testnext(ls, b';' as i32);
}

fn localstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip LOCAL
    if ls.t.token == TK_FUNCTION {
        // local function f() ... end
        ls.next_token();
        let name = check_name(ls);
        let _vidx = new_local(fs, name);
        adjust_locals(fs, 1);
        let mut e = ExprDesc::void();
        body(ls, fs, &mut e, false, ls.linenumber);
        // The closure is already at fs.freereg - 1 (from reserve_regs
        // in body()). That slot IS the local's register.
        return;
    }
    // Collect variable names: `a, b, c, ...` with optional <attrib>
    let mut names: Vec<(StringHandle, u8)> = Vec::new();
    loop {
        let name = check_name(ls);
        let kind = getlocalattrib(ls);
        names.push((name, kind));
        if !testnext(ls, b',' as i32) {
            break;
        }
    }
    let nvars = names.len();
    let first_vidx = fs.actvar.len();
    for &(name, _) in &names {
        let _vidx = new_local(fs, name);
    }
    for (i, (_, kind)) in names.iter().enumerate() {
        fs.actvar[first_vidx + i].kind = *kind;
    }
    let n_values = if testnext(ls, b'=' as i32) {
        // Collect all RHS expressions, then discharge with
        // multi-return expansion on the last call so
        // `local a, b, c = f()` captures all returns.
        let mut exprs: Vec<ExprDesc> = Vec::new();
        loop {
            let mut e = ExprDesc::void();
            expr(ls, fs, &mut e);
            exprs.push(e);
            if !testnext(ls, b',' as i32) {
                break;
            }
        }
        let n_exprs = exprs.len();
        let wanted = nvars as i32;
        for (i, e) in exprs.iter_mut().enumerate() {
            if i + 1 < n_exprs {
                lcode::exp2nextreg(fs, e);
            } else if e.k == ExpKind::Call && wanted > n_exprs as i32 {
                // Last expression is a call, caller wants more
                // values than we have — expand results.
                let call_pc = e.info as usize;
                let func_reg = ((fs.proto.code[call_pc] >> 7) & 0xFF) as u8;
                let extra = wanted - (n_exprs as i32 - 1);
                lcode::set_returns(fs, e, extra);
                fs.freereg = func_reg + extra as u8;
            } else {
                lcode::exp2nextreg(fs, e);
            }
        }
        // If last was a multi-return call, we count nvars'
        // worth of values (not n_exprs).
        let last_is_expanded_call = n_exprs >= 1
            && n_exprs < nvars
            && exprs.last().map(|e| e.k == ExpKind::Call).unwrap_or(false);
        if last_is_expanded_call {
            wanted
        } else {
            n_exprs as i32
        }
    } else {
        0
    };
    // Pad with nil up to nvars.
    for _ in (n_values as usize)..nvars {
        let mut e = ExprDesc::init(ExpKind::Nil, 0);
        lcode::exp2nextreg(fs, &mut e);
    }
    // Discard extra values (nvalues > nvars).
    if n_values as usize > nvars {
        fs.freereg = (fs.actvar.len() - (n_values as usize - nvars)) as u8;
    }
    adjust_locals(fs, nvars as i32);
}

fn exprstat(ls: &mut LexState, fs: &mut FuncState) {
    let mut e = ExprDesc::void();
    suffixedexp(ls, fs, &mut e);
    if ls.t.token == b'=' as i32 || ls.t.token == b',' as i32 {
        // Assignment.
        assignment(ls, fs, &mut e, 1);
    } else {
        // Function call (the expression is the statement).
        // Check that it's actually a call.
        if e.k != ExpKind::Call {
            ls.syntax_error("syntax error");
        }
        // Set result count to 0 — discard returns.
        lcode::set_returns(fs, &mut e, 0);
    }
}

fn assignment(ls: &mut LexState, fs: &mut FuncState, lhs: &mut ExprDesc, _nvars: i32) {
    if lhs.k == ExpKind::Local {
        let vidx = lhs.var.vidx as usize;
        if vidx < fs.actvar.len() && fs.actvar[vidx].kind == VDKCONST {
            ls.syntax_error("attempt to assign to const variable");
        }
    }
    check_next(ls, b'=' as i32);
    let mut rhs = ExprDesc::void();
    expr(ls, fs, &mut rhs);
    lcode::store_var(fs, lhs, &mut rhs);
}

fn ifstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip IF
    let mut e = ExprDesc::void();
    expr(ls, fs, &mut e);
    check_next(ls, TK_THEN);
    lcode::go_if_true(fs, &mut e);
    let false_jump = e.f;
    block(ls, fs);
    let mut escape_list = NO_JUMP;
    while ls.t.token == TK_ELSEIF {
        let ej = lcode::emit_jump(fs);
        lcode::concat_jmp(fs, &mut escape_list, ej);
        lcode::patch_to_here(fs, false_jump);
        ls.next_token(); // skip ELSEIF
        let mut cond = ExprDesc::void();
        expr(ls, fs, &mut cond);
        check_next(ls, TK_THEN);
        lcode::go_if_true(fs, &mut cond);
        let fb = cond.f;
        block(ls, fs);
        lcode::patch_to_here(fs, fb);
    }
    if ls.t.token == TK_ELSE {
        let ej = lcode::emit_jump(fs);
        lcode::concat_jmp(fs, &mut escape_list, ej);
        lcode::patch_to_here(fs, false_jump);
        ls.next_token(); // skip ELSE
        block(ls, fs);
    } else {
        lcode::patch_to_here(fs, false_jump);
    }
    lcode::patch_to_here(fs, escape_list);
    check_match(ls, TK_END, TK_IF);
}

fn whilestat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip WHILE
    let loop_start = lcode::get_label(fs);
    let mut e = ExprDesc::void();
    expr(ls, fs, &mut e);
    check_next(ls, TK_DO);
    lcode::go_if_true(fs, &mut e);
    let exit_jmp = e.f;
    // Open a loop block so `break` inside the body knows to
    // attach its JMP to this loop's break list.
    fs.blocks.push(BlockCnt {
        firstlabel: fs.labels.len(),
        firstgoto: fs.gotos.len(),
        nactvar: fs.nactvar,
        upval: false,
        is_loop: 1,
        inside_tbc: false,
        breaks: Vec::new(),
    });
    statlist(ls, fs);
    let bl = fs.blocks.pop().expect("while: block missing");
    // Back-edge jump to loop_start.
    let back_jmp = lcode::emit_jump(fs);
    lcode::patch_list(fs, back_jmp, loop_start);
    check_match(ls, TK_END, TK_WHILE);
    lcode::patch_to_here(fs, exit_jmp);
    // Patch all breaks to land here (after the back-edge).
    let here = lcode::get_label(fs);
    for brk in &bl.breaks {
        lcode::patch_list(fs, *brk, here);
    }
    fs.nactvar = bl.nactvar;
    fs.freereg = nvarstack(fs);
}

fn repeatstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip REPEAT
    let loop_start = lcode::get_label(fs);
    fs.blocks.push(BlockCnt {
        firstlabel: fs.labels.len(),
        firstgoto: fs.gotos.len(),
        nactvar: fs.nactvar,
        upval: false,
        is_loop: 1,
        inside_tbc: false,
        breaks: Vec::new(),
    });
    statlist(ls, fs);
    check_match(ls, TK_UNTIL, TK_REPEAT);
    let mut e = ExprDesc::void();
    expr(ls, fs, &mut e);
    lcode::go_if_true(fs, &mut e);
    lcode::patch_list(fs, e.f, loop_start);
    let bl = fs.blocks.pop().expect("repeat: block missing");
    let here = lcode::get_label(fs);
    for brk in &bl.breaks {
        lcode::patch_list(fs, *brk, here);
    }
    fs.nactvar = bl.nactvar;
    fs.freereg = nvarstack(fs);
}

fn forstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip FOR
    let name = check_name(ls);
    match ls.t.token {
        x if x == b'=' as i32 => fornum(ls, fs, name),
        x if x == b',' as i32 || x == TK_IN => forlist(ls, fs, name),
        _ => ls.syntax_error("'=' or 'in' expected"),
    }
}

/// Generic-for: `for v1, v2, ... in iter[, state[, control]] do body end`.
fn forlist(ls: &mut LexState, fs: &mut FuncState, first_var: StringHandle) {
    let base = fs.freereg;

    // Collect loop variable names (at least one, first_var).
    let mut var_names = vec![first_var];
    while testnext(ls, b',' as i32) {
        var_names.push(check_name(ls));
    }
    check_next(ls, TK_IN);

    // Layout (Lua 5.5):
    //   R(base+0) = iter (hidden)
    //   R(base+1) = state (hidden)
    //   R(base+2) = closing (hidden, stays nil)
    //   R(base+3) = first loop var (also serves as "control"
    //               variable — after TFORPREP swap, the initial
    //               control value ends up here and subsequent
    //               TFORCALLs overwrite it with their first
    //               return)
    //   R(base+4..) = additional loop vars
    let hidden = {
        let gs = unsafe { &mut *ls.gs };
        gs.new_string(b"(for state)", gs.hash_seed)
    };
    new_local(fs, hidden); // iterator func
    new_local(fs, hidden); // state
    new_local(fs, hidden); // closing
    // Do NOT adjust_locals here — during RHS parsing, these
    // slots are treated as temporaries so the compiler places
    // results directly at R(base), R(base+1), R(base+2).
    // Keep freereg at `base` so the RHS call lands at R(base).

    // Parse the RHS: 1..3 expressions (iter [, state [, control]]).
    // If only one expression and it's a function call, expand
    // up to 3 results. Otherwise each expression contributes 1.
    let mut exprs: Vec<ExprDesc> = Vec::new();
    loop {
        let mut e = ExprDesc::void();
        expr(ls, fs, &mut e);
        exprs.push(e);
        if !testnext(ls, b',' as i32) {
            break;
        }
    }
    let n_exprs = exprs.len();
    // The number of result "slots" we want to fill is 3 (iter,
    // state, control). Closing slot stays nil, filled below.
    let wanted = 3;
    // Discharge all-but-last as single-result. For the last, if
    // it's a call, request (wanted - n_exprs + 1) results so the
    // total count matches `wanted`.
    for (i, e) in exprs.iter_mut().enumerate() {
        if i + 1 < n_exprs {
            lcode::exp2nextreg(fs, e);
        } else {
            if e.k == ExpKind::Call {
                let extra = wanted - (n_exprs as i32 - 1);
                // Extract the function register from the A field
                // of the CALL instruction.
                let call_pc = e.info as usize;
                let func_reg = ((fs.proto.code[call_pc] >> 7) & 0xFF) as u8;
                lcode::set_returns(fs, e, extra);
                // After CALL with C = extra + 1, the results sit
                // at R(func_reg)..R(func_reg + extra - 1).
                // freereg should be func_reg + extra.
                fs.freereg = func_reg + extra as u8;
            } else {
                lcode::exp2nextreg(fs, e);
            }
        }
    }
    // Pad with nil up to base+4 — 3 hidden + the first loop
    // var slot (which receives the RHS's 3rd value as initial
    // control, or nil if ipairs/pairs only returned 2 values).
    while fs.freereg < base + 4 {
        let mut e = ExprDesc::init(ExpKind::Nil, 0);
        lcode::exp2nextreg(fs, &mut e);
    }
    fs.freereg = base + 4;
    check_next(ls, TK_DO);

    // Now activate the 3 hidden locals.
    adjust_locals(fs, 3);

    // TFORPREP skips to the TFORCALL at the end.
    let tforprep_pc = lcode::emit_abx(
        fs,
        crate::lopcodes::OpCode::OP_TFORPREP,
        base as u32,
        0,
    );

    // Declare loop variables (they live at R(base+3..)).
    for name in &var_names {
        new_local(fs, *name);
    }
    let nvars = var_names.len();
    adjust_locals(fs, nvars as i32);
    // Align freereg with actvar.len() so subsequent `local x = expr`
    // inside the body places x at actvar.len() consistently.
    fs.freereg = fs.actvar.len() as u8;

    // Loop body.
    fs.blocks.push(BlockCnt {
        firstlabel: fs.labels.len(),
        firstgoto: fs.gotos.len(),
        nactvar: fs.nactvar,
        upval: false,
        is_loop: 1,
        inside_tbc: false,
        breaks: Vec::new(),
    });
    let loop_body_start = fs.pc;
    statlist(ls, fs);
    check_match(ls, TK_END, TK_FOR);
    let bl = fs.blocks.pop().expect("forlist: block missing");

    // Patch TFORPREP Bx so it jumps forward to just before
    // TFORCALL (the next instruction after the FORPREP).
    let tforcall_pc = fs.pc as u32;
    let forprep_offset = tforcall_pc - (tforprep_pc as u32 + 1);
    let instr = &mut fs.proto.code[tforprep_pc as usize];
    *instr = (*instr & 0x7FFF) | (forprep_offset << 15);

    // TFORCALL: R(base+3)..R(base+3+nvars) := iter(state, ctrl).
    lcode::emit_abc(
        fs,
        crate::lopcodes::OpCode::OP_TFORCALL,
        base as u32,
        0,
        nvars as u32,
        false,
    );
    // TFORLOOP: if R(base+3) != nil then ctrl = R(base+3); pc -= Bx.
    let back_offset = (fs.pc + 1 - loop_body_start as i32) as u32;
    lcode::emit_abx(
        fs,
        crate::lopcodes::OpCode::OP_TFORLOOP,
        base as u32,
        back_offset,
    );

    // Patch breaks to land post-loop.
    let here = lcode::get_label(fs);
    for brk in &bl.breaks {
        lcode::patch_list(fs, *brk, here);
    }

    // Clean up: pop the 3 hidden + nvars loop-variable locals.
    let total = 3 + nvars as i16;
    fs.nactvar -= total;
    fs.actvar.truncate(fs.actvar.len() - total as usize);
    fs.freereg = base;
}

fn fornum(ls: &mut LexState, fs: &mut FuncState, loop_var: StringHandle) {
    ls.next_token(); // skip '='
    let base = fs.freereg;
    // R(A)   = init
    // R(A+1) = limit
    // R(A+2) = step
    // R(A+3) = loop variable (user-visible)
    //
    // The three control slots are hidden pseudo-locals so the
    // register allocator doesn't stomp them. We push synthetic
    // names.
    let hidden = {
        let gs = unsafe { &mut *ls.gs };
        gs.new_string(b"(for state)", gs.hash_seed)
    };
    new_local(fs, hidden); // init slot
    new_local(fs, hidden); // limit slot
    new_local(fs, hidden); // step slot
    adjust_locals(fs, 3);
    fs.freereg = base + 3;

    // initial
    let mut e = ExprDesc::void();
    expr(ls, fs, &mut e);
    lcode::exp2nextreg_at(fs, &mut e, base);
    check_next(ls, b',' as i32);
    // limit
    expr(ls, fs, &mut e);
    lcode::exp2nextreg_at(fs, &mut e, base + 1);
    if testnext(ls, b',' as i32) {
        expr(ls, fs, &mut e);
        lcode::exp2nextreg_at(fs, &mut e, base + 2);
    } else {
        // default step = 1
        let mut step = ExprDesc::init(ExpKind::KInt, 0);
        step.ival = 1;
        lcode::exp2nextreg_at(fs, &mut step, base + 2);
    }
    check_next(ls, TK_DO);

    // Emit OP_FORPREP. Bx will be patched after we emit the body.
    let forprep_pc = lcode::emit_abx(
        fs,
        crate::lopcodes::OpCode::OP_FORPREP,
        base as u32,
        0,
    );

    // Declare the loop variable at R(A+3).
    new_local(fs, loop_var);
    adjust_locals(fs, 1);
    fs.freereg = base + 4;

    // Open a loop block so `break` lands after the FORLOOP.
    fs.blocks.push(BlockCnt {
        firstlabel: fs.labels.len(),
        firstgoto: fs.gotos.len(),
        nactvar: fs.nactvar,
        upval: false,
        is_loop: 1,
        inside_tbc: false,
        breaks: Vec::new(),
    });
    let loop_start = fs.pc;
    statlist(ls, fs);
    check_match(ls, TK_END, TK_FOR);
    let bl = fs.blocks.pop().expect("for: block missing");

    // Emit OP_FORLOOP jumping back to loop_start.
    let back_offset = (fs.pc - loop_start + 1) as u32; // +1 for FORLOOP itself
    lcode::emit_abx(
        fs,
        crate::lopcodes::OpCode::OP_FORLOOP,
        base as u32,
        back_offset,
    );

    // Patch FORPREP to jump past body + FORLOOP.
    let skip_offset = (fs.pc - forprep_pc - 1) as u32;
    let instr = &mut fs.proto.code[forprep_pc as usize];
    // Replace Bx. Bx is bits 15..31 (17 bits).
    *instr = (*instr & 0x7FFF) | (skip_offset << 15);

    // Patch breaks inside the for body to land here
    // (post-FORLOOP).
    let here = lcode::get_label(fs);
    for brk in &bl.breaks {
        lcode::patch_list(fs, *brk, here);
    }

    // Clean up: pop the 4 for-loop locals.
    fs.nactvar -= 4;
    fs.actvar.truncate(fs.actvar.len() - 4);
    fs.freereg = base;
}

fn funcstat(ls: &mut LexState, fs: &mut FuncState) {
    let line = ls.linenumber;
    ls.next_token(); // skip FUNCTION
    // Parse the target: name [ . name ]* [ : method ]
    let mut v = ExprDesc::void();
    let name = check_name(ls);
    singlevar(ls, fs, &mut v, name);
    while ls.t.token == b'.' as i32 {
        ls.next_token();
        let field_name = check_name(ls);
        lcode::field_access(fs, &mut v, field_name);
    }
    // Method-definition colon: 'function T:m(...)' is sugar for
    // 'function T.m(self, ...)'.
    let is_method = ls.t.token == b':' as i32;
    if is_method {
        ls.next_token();
        let method_name = check_name(ls);
        lcode::field_access(fs, &mut v, method_name);
    }
    let mut body_e = ExprDesc::void();
    body(ls, fs, &mut body_e, is_method, line);
    lcode::store_var(fs, &mut v, &mut body_e);
}

/// Parse a function body: `( [params] ) [block] end`.
/// `is_method` (true for `function obj:m()`) adds `self` as the
/// first implicit parameter.
fn body(ls: &mut LexState, outer_fs: &mut FuncState, e: &mut ExprDesc, is_method: bool, line: i32) {
    // Save the inner proto handle idx in outer's inner_protos.
    let inner_proto_idx = outer_fs.proto.inner_protos.len();

    // Build the inner function.
    let mut inner_fs = FuncState::new();
    inner_fs.proto.line_defined = line;
    // Inner function inherits outer's locals as potential
    // upvalues. We snapshot outer locals here so singlevar can
    // resolve names and create upvalues lazily.
    inner_fs.outer_locals = outer_fs.actvar.clone();
    inner_fs.outer_nactvar = outer_fs.nactvar;

    check_next(ls, b'(' as i32);
    let mut num_params: u8 = 0;
    if is_method {
        // Implicit 'self' parameter.
        let self_name = {
            let gs = unsafe { &mut *ls.gs };
            gs.new_string(b"self", gs.hash_seed)
        };
        new_local(&mut inner_fs, self_name);
        num_params += 1;
    }
    if ls.t.token != b')' as i32 {
        loop {
            match ls.t.token {
                TK_NAME => {
                    let pname = check_name(ls);
                    new_local(&mut inner_fs, pname);
                    num_params += 1;
                }
                TK_DOTS => {
                    ls.next_token();
                    inner_fs.proto.is_vararg = true;
                    break;
                }
                _ => ls.syntax_error("name or '...' expected"),
            }
            if !testnext(ls, b',' as i32) {
                break;
            }
        }
    }
    check_next(ls, b')' as i32);

    inner_fs.proto.num_params = num_params;
    inner_fs.nactvar = num_params as i16;
    inner_fs.freereg = num_params;
    inner_fs.proto.max_stack_size = num_params.max(2);

    // Open the main block.
    inner_fs.blocks.push(BlockCnt {
        firstlabel: 0,
        firstgoto: 0,
        nactvar: num_params as i16,
        upval: false,
        is_loop: 0,
        inside_tbc: false,
        breaks: Vec::new(),
    });

    // Parse the body.
    if inner_fs.proto.is_vararg {
        // Emit OP_VARARGPREP at the start of vararg functions.
        lcode::emit_abc(
            &mut inner_fs,
            crate::lopcodes::OpCode::OP_VARARGPREP,
            num_params as u32,
            0,
            0,
            false,
        );
    }
    statlist(ls, &mut inner_fs);
    check_match(ls, TK_END, TK_FUNCTION);

    // Emit final RETURN0 if the body didn't end with RETURN.
    lcode::code_return(&mut inner_fs, 0, 0);

    inner_fs.proto.last_line_defined = ls.linenumber;
    inner_fs.proto.max_stack_size = inner_fs.proto.max_stack_size.max(inner_fs.freereg);

    // Allocate the inner proto on the heap.
    let inner_proto = {
        let gs = unsafe { &mut *ls.gs };
        gs.heap.alloc_proto(inner_fs.proto)
    };
    outer_fs.proto.inner_protos.push(inner_proto);

    // Emit OP_CLOSURE in the outer function.
    let reg = outer_fs.freereg;
    let pc = lcode::emit_abx(
        outer_fs,
        crate::lopcodes::OpCode::OP_CLOSURE,
        reg as u32,
        inner_proto_idx as u32,
    );
    lcode::reserve_regs(outer_fs, 1);
    e.k = ExpKind::NonReloc;
    e.info = reg as i32;
    let _ = pc;
}

fn block(ls: &mut LexState, fs: &mut FuncState) {
    fs.blocks.push(BlockCnt {
        firstlabel: fs.labels.len(),
        firstgoto: fs.gotos.len(),
        nactvar: fs.nactvar,
        upval: false,
        is_loop: 0,
        inside_tbc: false,
        breaks: Vec::new(),
    });
    statlist(ls, fs);
    let bl = fs.blocks.pop().unwrap();
    // Remove locals declared in this block.
    fs.nactvar = bl.nactvar;
    fs.freereg = nvarstack(fs);
}

// ---- Expression parsing ----------------------------------------------------

/// Parse an expression list, leaving results in consecutive
/// registers starting at `fs.freereg`. Returns number of
/// expressions.
fn explist(ls: &mut LexState, fs: &mut FuncState, ) -> i32 {
    let mut n = 1;
    let mut e = ExprDesc::void();
    expr(ls, fs, &mut e);
    while testnext(ls, b',' as i32) {
        lcode::exp2nextreg(fs, &mut e);
        e = ExprDesc::void();
        expr(ls, fs, &mut e);
        n += 1;
    }
    lcode::exp2nextreg(fs, &mut e);
    n
}

fn expr(ls: &mut LexState, fs: &mut FuncState, e: &mut ExprDesc) {
    subexpr(ls, fs, e, 0);
}

/// Operator precedence table. Returns (left priority, right priority).
fn get_bin_priority(op: BinOpr) -> (u8, u8) {
    match op {
        BinOpr::Or => (1, 1),
        BinOpr::And => (2, 2),
        BinOpr::Lt | BinOpr::Gt | BinOpr::Le | BinOpr::Ge
        | BinOpr::Ne | BinOpr::Eq => (3, 3),
        BinOpr::BOr => (4, 4),
        BinOpr::BXor => (5, 5),
        BinOpr::BAnd => (6, 6),
        BinOpr::Shl | BinOpr::Shr => (7, 7),
        BinOpr::Concat => (8, 7), // right-associative
        BinOpr::Add | BinOpr::Sub => (10, 10),
        BinOpr::Mul | BinOpr::Mod | BinOpr::IDiv => (11, 11),
        BinOpr::Div => (11, 11),
        BinOpr::Pow => (14, 13), // right-associative
        BinOpr::NoBinOpr => (0, 0),
    }
}

const UNARY_PRIORITY: u8 = 12;

fn subexpr(ls: &mut LexState, fs: &mut FuncState, e: &mut ExprDesc, limit: u8) {
    let uop = get_unary_op(ls.t.token);
    if uop != UnOpr::NoUnOpr {
        let line = ls.linenumber;
        ls.next_token();
        subexpr(ls, fs, e, UNARY_PRIORITY);
        lcode::prefix(fs, uop, e, line);
    } else {
        simpleexp(ls, fs, e);
    }
    let mut op = get_binary_op(ls.t.token);
    while op != BinOpr::NoBinOpr {
        let (left, right) = get_bin_priority(op);
        if left <= limit {
            break;
        }
        ls.next_token();
        lcode::infix(fs, op, e);
        let mut e2 = ExprDesc::void();
        subexpr(ls, fs, &mut e2, right);
        lcode::posfix(fs, op, e, &mut e2);
        op = get_binary_op(ls.t.token);
    }
}

fn simpleexp(ls: &mut LexState, fs: &mut FuncState, e: &mut ExprDesc) {
    match ls.t.token {
        TK_INT => {
            e.k = ExpKind::KInt;
            e.ival = match &ls.t.seminfo {
                SemInfo::Integer(i) => *i,
                _ => 0,
            };
            e.t = NO_JUMP;
            e.f = NO_JUMP;
            ls.next_token();
        }
        TK_FLT => {
            e.k = ExpKind::KFlt;
            e.nval = match &ls.t.seminfo {
                SemInfo::Float(f) => *f,
                _ => 0.0,
            };
            e.t = NO_JUMP;
            e.f = NO_JUMP;
            ls.next_token();
        }
        TK_STRING => {
            let h = match &ls.t.seminfo {
                SemInfo::String(h) => *h,
                _ => panic!("TK_STRING without StringHandle"),
            };
            e.k = ExpKind::KStr;
            e.strval = Some(h);
            e.t = NO_JUMP;
            e.f = NO_JUMP;
            ls.next_token();
        }
        TK_NIL => {
            *e = ExprDesc::init(ExpKind::Nil, 0);
            ls.next_token();
        }
        TK_TRUE => {
            *e = ExprDesc::init(ExpKind::True, 0);
            ls.next_token();
        }
        TK_FALSE => {
            *e = ExprDesc::init(ExpKind::False, 0);
            ls.next_token();
        }
        TK_DOTS => {
            *e = ExprDesc::init(ExpKind::VarArg, 0);
            lcode::code_vararg(fs, e);
            ls.next_token();
        }
        x if x == b'{' as i32 => {
            constructor(ls, fs, e);
        }
        TK_FUNCTION => {
            ls.next_token();
            body(ls, fs, e, false, ls.linenumber);
        }
        _ => {
            suffixedexp(ls, fs, e);
        }
    }
}

/// Table constructor: `{ [fields] }`.
/// Each field is either:
///   - `name = expr` → set name key
///   - `[expr] = expr` → set expression key
///   - `expr` → array-style positional field
fn constructor(ls: &mut LexState, fs: &mut FuncState, e: &mut ExprDesc) {
    ls.next_token(); // skip '{'
    let reg = fs.freereg as u32;
    // Emit OP_NEWTABLE. A = reg, B = array hint, C = hash hint,
    // followed by an OP_EXTRAARG.
    lcode::emit_abc(fs, crate::lopcodes::OpCode::OP_NEWTABLE, reg, 0, 0, false);
    lcode::emit(
        fs,
        crate::lopcodes::create_abx(crate::lopcodes::OpCode::OP_EXTRAARG, 0, 0),
    );
    lcode::reserve_regs(fs, 1);

    let mut array_count: u32 = 0; // positional fields seen
    let mut array_pending: u32 = 0; // positional fields awaiting SETLIST
    while ls.t.token != b'}' as i32 {
        if ls.t.token == b'[' as i32 {
            // [expr] = expr
            ls.next_token();
            let mut key = ExprDesc::void();
            expr(ls, fs, &mut key);
            check_next(ls, b']' as i32);
            check_next(ls, b'=' as i32);
            let mut val = ExprDesc::void();
            expr(ls, fs, &mut val);
            let mut target = ExprDesc::init(ExpKind::NonReloc, reg as i32);
            lcode::indexed(fs, &mut target, &mut key);
            lcode::store_var(fs, &mut target, &mut val);
            fs.freereg = (reg + 1 + array_pending) as u8;
        } else if ls.t.token == TK_NAME
            && peek_next_is_equals(ls)
        {
            // name = expr
            let name = check_name(ls);
            check_next(ls, b'=' as i32);
            let mut val = ExprDesc::void();
            expr(ls, fs, &mut val);
            let mut target = ExprDesc::init(ExpKind::NonReloc, reg as i32);
            lcode::field_access(fs, &mut target, name);
            lcode::store_var(fs, &mut target, &mut val);
            fs.freereg = (reg + 1 + array_pending) as u8;
        } else {
            // positional
            let mut val = ExprDesc::void();
            expr(ls, fs, &mut val);
            lcode::exp2nextreg(fs, &mut val);
            array_count += 1;
            array_pending += 1;
        }
        if !testnext(ls, b',' as i32) && !testnext(ls, b';' as i32) {
            break;
        }
    }
    check_match(ls, b'}' as i32, b'{' as i32);

    if array_pending > 0 {
        // Emit SETLIST to move positional fields into the table.
        lcode::emit(
            fs,
            crate::lopcodes::create_vabck(
                crate::lopcodes::OpCode::OP_SETLIST,
                reg,
                array_pending,
                0,
                false,
            ),
        );
        fs.freereg = (reg + 1) as u8;
        let _ = array_count;
    }

    *e = ExprDesc::init(ExpKind::NonReloc, reg as i32);
}

/// Peek whether the token *after* the current TK_NAME is `=`.
/// Used to disambiguate `{name = expr}` from `{name}`.
fn peek_next_is_equals(ls: &mut LexState) -> bool {
    let saved = ls.t.clone();
    ls.lookahead = crate::llex::Token::eos();
    let next = ls.lookahead_token();
    // Don't consume the lookahead; it remains in ls.lookahead
    // for next_token() to pick up.
    let _ = saved;
    next == b'=' as i32
}

fn suffixedexp(ls: &mut LexState, fs: &mut FuncState, e: &mut ExprDesc) {
    primaryexp(ls, fs, e);
    loop {
        match ls.t.token {
            t if t == b'.' as i32 => {
                ls.next_token();
                let name = check_name(ls);
                lcode::field_access(fs, e, name);
            }
            t if t == b'[' as i32 => {
                lcode::exp2anyreg(fs, e);
                ls.next_token();
                let mut key = ExprDesc::void();
                expr(ls, fs, &mut key);
                lcode::indexed(fs, e, &mut key);
                check_next(ls, b']' as i32);
            }
            t if t == b'(' as i32 => {
                lcode::exp2nextreg(fs, e);
                ls.next_token();
                let nargs = if ls.t.token == b')' as i32 {
                    0
                } else {
                    explist(ls, fs)
                };
                check_next(ls, b')' as i32);
                lcode::code_call(fs, e, nargs);
            }
            t if t == b':' as i32 => {
                // Method call: e:method(args) compiles to
                //   SELF R(A), e_reg, K[method]  — puts method
                //     in R(A) and e in R(A+1) (as 'self').
                //   CALL R(A), B, C
                ls.next_token();
                let name = check_name(ls);
                lcode::self_call(fs, e, name);
                // Expect '(' after :method to start call args.
                match ls.t.token {
                    tk if tk == b'(' as i32 => {
                        ls.next_token();
                        let extra = if ls.t.token == b')' as i32 {
                            0
                        } else {
                            explist(ls, fs)
                        };
                        check_next(ls, b')' as i32);
                        // Add 1 for the implicit self arg.
                        lcode::code_call(fs, e, extra + 1);
                    }
                    tk if tk == b'{' as i32 => {
                        let mut tbl = ExprDesc::void();
                        constructor(ls, fs, &mut tbl);
                        lcode::exp2nextreg(fs, &mut tbl);
                        lcode::code_call(fs, e, 2);
                    }
                    TK_STRING => {
                        let h = match &ls.t.seminfo {
                            SemInfo::String(h) => *h,
                            _ => panic!("TK_STRING without handle"),
                        };
                        ls.next_token();
                        let mut arg = ExprDesc::void();
                        arg.k = ExpKind::KStr;
                        arg.strval = Some(h);
                        arg.t = NO_JUMP;
                        arg.f = NO_JUMP;
                        lcode::exp2nextreg(fs, &mut arg);
                        lcode::code_call(fs, e, 2);
                    }
                    _ => ls.syntax_error("function arguments expected"),
                }
            }
            TK_STRING => {
                // f"str" sugar
                lcode::exp2nextreg(fs, e);
                let h = match &ls.t.seminfo {
                    SemInfo::String(h) => *h,
                    _ => panic!("TK_STRING without handle"),
                };
                ls.next_token();
                let mut arg = ExprDesc::void();
                arg.k = ExpKind::KStr;
                arg.strval = Some(h);
                arg.t = NO_JUMP;
                arg.f = NO_JUMP;
                lcode::exp2nextreg(fs, &mut arg);
                lcode::code_call(fs, e, 1);
            }
            _ => return,
        }
    }
}

fn primaryexp(ls: &mut LexState, fs: &mut FuncState, e: &mut ExprDesc) {
    match ls.t.token {
        TK_NAME => {
            let name = match &ls.t.seminfo {
                SemInfo::String(h) => *h,
                _ => panic!("TK_NAME without StringHandle"),
            };
            ls.next_token();
            singlevar(ls, fs, e, name);
        }
        t if t == b'(' as i32 => {
            ls.next_token();
            expr(ls, fs, e);
            check_next(ls, b')' as i32);
            lcode::discharge_vars(fs, e);
        }
        _ => {
            ls.syntax_error("unexpected symbol");
        }
    }
}

fn singlevar(_ls: &mut LexState, fs: &mut FuncState, e: &mut ExprDesc, name: StringHandle) {
    // Search locals.
    for i in (0..fs.nactvar as usize).rev() {
        let av = &fs.actvar[i];
        if av.name == Some(name) {
            *e = ExprDesc::init(ExpKind::Local, 0);
            e.var.ridx = av.ridx;
            e.var.vidx = i as i16;
            return;
        }
    }
    // Search existing upvalues (already captured).
    for i in 0..fs.nups as usize {
        let uv = &fs.proto.upvalues[i];
        if uv.name == Some(name) {
            *e = ExprDesc::init(ExpKind::Upval, i as i32);
            return;
        }
    }
    // Search the outer function's locals — capture as upvalue.
    for i in (0..fs.outer_nactvar as usize).rev() {
        let av = &fs.outer_locals[i];
        if av.name == Some(name) {
            let upv_idx = fs.proto.upvalues.len();
            fs.proto.upvalues.push(UpvalDesc {
                name: Some(name),
                in_stack: true,
                idx: av.ridx,
                kind: 0,
            });
            fs.nups += 1;
            *e = ExprDesc::init(ExpKind::Upval, upv_idx as i32);
            return;
        }
    }
    // Fall back: global via _ENV[name]. Convention: _ENV lives at
    // upvalue slot 0. The main chunk pre-registers it; nested
    // functions chain through by referencing the outer's
    // upvalue 0 when they lack a local binding.
    let env_idx: i32 = if !fs.proto.upvalues.is_empty()
        && fs.proto.upvalues[0].name.is_none()
    {
        0
    } else {
        fs.proto.upvalues.push(UpvalDesc {
            name: None,
            in_stack: false,
            idx: 0,
            kind: 0,
        });
        fs.nups += 1;
        (fs.proto.upvalues.len() - 1) as i32
    };
    *e = ExprDesc::init(ExpKind::Upval, env_idx);
    let mut key = ExprDesc::void();
    key.k = ExpKind::KStr;
    key.strval = Some(name);
    key.t = NO_JUMP;
    key.f = NO_JUMP;
    lcode::indexed(fs, e, &mut key);
}

fn is_env_upval(u: &UpvalDesc) -> bool {
    u.name.is_none() && !u.in_stack && u.idx == 0
}

// ---- Operator mapping ------------------------------------------------------

fn get_unary_op(token: i32) -> UnOpr {
    match token {
        TK_NOT => UnOpr::Not,
        t if t == b'-' as i32 => UnOpr::Minus,
        t if t == b'~' as i32 => UnOpr::BNot,
        t if t == b'#' as i32 => UnOpr::Len,
        _ => UnOpr::NoUnOpr,
    }
}

fn get_binary_op(token: i32) -> BinOpr {
    match token {
        t if t == b'+' as i32 => BinOpr::Add,
        t if t == b'-' as i32 => BinOpr::Sub,
        t if t == b'*' as i32 => BinOpr::Mul,
        t if t == b'%' as i32 => BinOpr::Mod,
        t if t == b'^' as i32 => BinOpr::Pow,
        t if t == b'/' as i32 => BinOpr::Div,
        TK_IDIV => BinOpr::IDiv,
        t if t == b'&' as i32 => BinOpr::BAnd,
        t if t == b'|' as i32 => BinOpr::BOr,
        t if t == b'~' as i32 => BinOpr::BXor,
        TK_SHL => BinOpr::Shl,
        TK_SHR => BinOpr::Shr,
        TK_CONCAT => BinOpr::Concat,
        TK_NE => BinOpr::Ne,
        TK_EQ => BinOpr::Eq,
        t if t == b'<' as i32 => BinOpr::Lt,
        TK_LE => BinOpr::Le,
        t if t == b'>' as i32 => BinOpr::Gt,
        TK_GE => BinOpr::Ge,
        TK_AND => BinOpr::And,
        TK_OR => BinOpr::Or,
        _ => BinOpr::NoBinOpr,
    }
}

// ---- Helpers ---------------------------------------------------------------

fn testnext(ls: &mut LexState, c: i32) -> bool {
    if ls.t.token == c {
        ls.next_token();
        true
    } else {
        false
    }
}

fn check_next(ls: &mut LexState, c: i32) {
    if ls.t.token != c {
        ls.syntax_error(&format!("'{}' expected", c as u8 as char));
    }
    ls.next_token();
}

fn check_match(ls: &mut LexState, what: i32, _who: i32) {
    if !testnext(ls, what) {
        ls.syntax_error(&format!("'{}' expected", token_name(what)));
    }
}

fn check_name(ls: &mut LexState) -> StringHandle {
    if ls.t.token != TK_NAME {
        ls.syntax_error("name expected");
    }
    let h = match &ls.t.seminfo {
        SemInfo::String(h) => *h,
        _ => ls.syntax_error("name expected"),
    };
    ls.next_token();
    h
}

/// Parse optional `<Name>` attribute after a local-var name.
/// Returns VDKREG, VDKCONST, or VDKTOCLOSE.
fn getlocalattrib(ls: &mut LexState) -> u8 {
    if !testnext(ls, b'<' as i32) {
        return VDKREG;
    }
    let attr_h = check_name(ls);
    check_next(ls, b'>' as i32);
    let bytes = unsafe { (*ls.gs).heap.string(attr_h).bytes.clone() };
    let kind = match bytes.as_slice() {
        b"const" => VDKCONST,
        b"close" => VDKTOCLOSE,
        _ => ls.syntax_error("unknown attribute"),
    };
    kind
}

fn token_name(t: i32) -> &'static str {
    match t {
        TK_END => "end",
        TK_DO => "do",
        TK_THEN => "then",
        TK_IF => "if",
        TK_WHILE => "while",
        TK_REPEAT => "repeat",
        TK_UNTIL => "until",
        TK_FOR => "for",
        _ => "?",
    }
}

fn new_local(fs: &mut FuncState, name: StringHandle) -> usize {
    // Assign the next-free register to this local. For function
    // parameters we call new_local before bumping nactvar, so the
    // ridx is the position in the actvar list (index).
    let ridx = fs.actvar.len() as u8;
    let idx = fs.actvar.len();
    fs.actvar.push(ActiveVar {
        name: Some(name),
        kind: VDKREG,
        ridx,
        pidx: -1,
    });
    idx
}

fn adjust_locals(fs: &mut FuncState, nvars: i32) {
    fs.nactvar += nvars as i16;
}

fn nvarstack(fs: &FuncState) -> u8 {
    let mut reg: u8 = 0;
    for i in 0..fs.nactvar as usize {
        if i < fs.actvar.len() {
            reg = reg.max(fs.actvar[i].ridx + 1);
        }
    }
    reg
}

// ---- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_return_integer() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"return 42", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn parse_return_addition() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"return 1 + 2", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(3));
    }

    #[test]
    fn parse_return_arithmetic_precedence() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"return 2 + 3 * 4", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(14));
    }

    #[test]
    fn parse_local_and_return() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"local x = 10\nreturn x", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(10));
    }

    #[test]
    fn parse_negation() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"return -5", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(-5));
    }

    #[test]
    fn parse_if_then_else_true_branch() {
        let mut state = LuaState::new(0);
        let src = b"local x = 10\nif x > 5 then return 1 else return 0 end";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn parse_if_then_else_false_branch() {
        let mut state = LuaState::new(0);
        let src = b"local x = 3\nif x > 5 then return 1 else return 0 end";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(0));
    }

    #[test]
    fn parse_table_constructor_empty() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"local t = {}; return t[1]", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        // t[1] on an empty table returns nil.
        assert!(state.is_nil(1));
    }

    #[test]
    fn parse_table_positional_fields() {
        let mut state = LuaState::new(0);
        let closure =
            parse(&mut state, b"local t = {10, 20, 30}; return t[2]", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(20));
    }

    #[test]
    fn parse_table_named_field() {
        let mut state = LuaState::new(0);
        let closure =
            parse(&mut state, b"local t = {x = 42}; return t.x", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn parse_local_function_and_call() {
        let mut state = LuaState::new(0);
        let src = b"local function f(x) return x + 1 end\nreturn f(41)";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn parse_function_with_two_args() {
        let mut state = LuaState::new(0);
        let src = b"local function add(a, b) return a + b end\nreturn add(10, 32)";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn parse_recursive_factorial() {
        let mut state = LuaState::new(0);
        let src = b"local function fact(n)\n  if n <= 1 then return 1 else return n * fact(n - 1) end\nend\nreturn fact(5)";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(120));
    }

    #[test]
    fn parse_recursive_fibonacci() {
        let mut state = LuaState::new(0);
        let src = b"local function fib(n)\n  if n < 2 then return n else return fib(n-1) + fib(n-2) end\nend\nreturn fib(10)";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(55));
    }

    #[test]
    fn parse_and_call_print_via_env() {
        use crate::lbaselib::{open_base, reset_print_buffer, take_print_buffer};
        let mut state = LuaState::new(0);
        let globals = open_base(&mut state);
        reset_print_buffer();
        let closure = parse_with_env(&mut state, b"print(42)", b"=test", globals);
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        match state.call_value(0, 0, 0) {
            Ok(()) => {}
            Err(e) => {
                if let crate::contract::LuaError::Runtime(TValue::ShortString(h))
                    | crate::contract::LuaError::Runtime(TValue::LongString(h)) = e
                {
                    let bytes = state.global.heap.string(h).bytes.clone();
                    panic!("runtime error: {}", String::from_utf8_lossy(&bytes));
                } else {
                    panic!("runtime error: {:?}", e);
                }
            }
        }
        let buf = take_print_buffer();
        assert_eq!(buf, vec!["42".to_string()]);
    }

    #[test]
    fn parse_tostring_type_check() {
        use crate::lbaselib::{open_base, reset_print_buffer, take_print_buffer};
        let mut state = LuaState::new(0);
        let globals = open_base(&mut state);
        reset_print_buffer();
        let closure = parse_with_env(
            &mut state,
            b"print(type(42))",
            b"=test",
            globals,
        );
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 0).unwrap();
        assert_eq!(take_print_buffer(), vec!["number".to_string()]);
    }

    #[test]
    fn parse_iterative_fibonacci() {
        // fib iteratively up to 10 terms.
        let mut state = LuaState::new(0);
        let src = b"local a, b = 0, 1\nfor i = 1, 10 do local t = a + b; a = b; b = t end\nreturn a";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(55));
    }

    #[test]
    fn parse_string_concat() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"return \"hello\" .. \" \" .. \"world\"", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_lstring(1), Some(b"hello world".as_slice()));
    }

    #[test]
    fn parse_numeric_for_loop() {
        let mut state = LuaState::new(0);
        let src = b"local sum = 0\nfor i = 1, 10 do sum = sum + i end\nreturn sum";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(55));
    }

    #[test]
    fn parse_string_literal_return() {
        let mut state = LuaState::new(0);
        let closure = parse(&mut state, b"return \"hello\"", b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_lstring(1), Some(b"hello".as_slice()));
    }

    #[test]
    fn parse_while_loop() {
        let mut state = LuaState::new(0);
        let src = b"local i = 0\nlocal sum = 0\nwhile i < 5 do sum = sum + i; i = i + 1 end\nreturn sum";
        let closure = parse(&mut state, src, b"=test");
        state.current_thread_mut().push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(10)); // 0+1+2+3+4
    }
}
