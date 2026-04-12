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
    LexState, SemInfo, TK_AND, TK_BREAK, TK_CONCAT, TK_DO, TK_DOTS,
    TK_ELSE, TK_ELSEIF, TK_END, TK_EOS, TK_EQ, TK_FALSE, TK_FOR, TK_FUNCTION,
    TK_GE, TK_IDIV, TK_IF, TK_INT, TK_LE, TK_LOCAL, TK_NAME,
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

// ---- Block scope -----------------------------------------------------------

#[derive(Debug)]
pub struct BlockCnt {
    pub firstlabel: usize,
    pub firstgoto: usize,
    pub nactvar: i16,
    pub upval: bool,
    pub is_loop: u8,
    pub inside_tbc: bool,
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
        }
    }
}

// ---- Parser entry ----------------------------------------------------------

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
        TK_BREAK => {
            ls.next_token();
            // break is a placeholder — needs label resolution.
            // For now just emit a JMP that will need patching.
            let jmp = lcode::emit_jump(fs);
            // TODO: record break for patching when block closes.
            let _ = jmp;
        }
        x if x == b';' as i32 => {
            ls.next_token(); // skip ';'
        }
        _ => exprstat(ls, fs),
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
    lcode::code_return(fs, first as i32, nret);
    // Optional semicolon.
    testnext(ls, b';' as i32);
}

fn localstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip LOCAL
    let name = check_name(ls);
    let vidx = new_local(fs, name);
    if testnext(ls, b'=' as i32) {
        let mut e = ExprDesc::void();
        expr(ls, fs, &mut e);
        lcode::exp2nextreg(fs, &mut e);
    } else {
        // local x (no initializer) → nil.
        let mut e = ExprDesc::init(ExpKind::Nil, 0);
        lcode::exp2nextreg(fs, &mut e);
    }
    adjust_locals(fs, 1);
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
    // Simple single-variable assignment for now.
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
    block(ls, fs);
    let back_jmp = lcode::emit_jump(fs);
    lcode::patch_list(fs, back_jmp, loop_start);
    check_match(ls, TK_END, TK_WHILE);
    lcode::patch_to_here(fs, exit_jmp);
}

fn repeatstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip REPEAT
    let loop_start = lcode::get_label(fs);
    block(ls, fs);
    check_match(ls, TK_UNTIL, TK_REPEAT);
    let mut e = ExprDesc::void();
    expr(ls, fs, &mut e);
    lcode::go_if_true(fs, &mut e);
    lcode::patch_list(fs, e.f, loop_start);
}

fn forstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip FOR
    let _name = check_name(ls);
    match ls.t.token {
        x if x == b'=' as i32 => fornum(ls, fs),
        _ => ls.syntax_error("'=' or 'in' expected"),
    }
}

fn fornum(ls: &mut LexState, fs: &mut FuncState) {
    // For now, a simplified numeric for.
    ls.next_token(); // skip '='
    // initial
    let mut e = ExprDesc::void();
    expr(ls, fs, &mut e);
    lcode::exp2nextreg(fs, &mut e);
    check_next(ls, b',' as i32);
    // limit
    expr(ls, fs, &mut e);
    lcode::exp2nextreg(fs, &mut e);
    if testnext(ls, b',' as i32) {
        // step
        expr(ls, fs, &mut e);
        lcode::exp2nextreg(fs, &mut e);
    } else {
        // default step = 1
        let mut step = ExprDesc::init(ExpKind::KInt, 0);
        step.ival = 1;
        lcode::exp2nextreg(fs, &mut step);
    }
    check_next(ls, TK_DO);
    // Body (simplified — no proper for-loop opcodes yet from parser side).
    block(ls, fs);
    check_match(ls, TK_END, TK_FOR);
}

fn funcstat(ls: &mut LexState, fs: &mut FuncState) {
    ls.next_token(); // skip FUNCTION
    // Simplified: just parse "function name() ... end" as assignment.
    let mut v = ExprDesc::void();
    let name = check_name(ls);
    singlevar(ls, fs, &mut v, name);
    // TODO: body parsing
    ls.syntax_error("function definitions not yet implemented in parser");
}

fn block(ls: &mut LexState, fs: &mut FuncState) {
    fs.blocks.push(BlockCnt {
        firstlabel: fs.labels.len(),
        firstgoto: fs.gotos.len(),
        nactvar: fs.nactvar,
        upval: false,
        is_loop: 0,
        inside_tbc: false,
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
            // Table constructor — simplified stub.
            ls.syntax_error("table constructors not yet implemented");
        }
        TK_FUNCTION => {
            ls.syntax_error("function expressions not yet implemented");
        }
        _ => {
            suffixedexp(ls, fs, e);
        }
    }
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
                ls.next_token();
                let _method = check_name(ls);
                ls.syntax_error("method calls not yet implemented");
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
    // Search upvalues.
    for i in 0..fs.nups as usize {
        let uv = &fs.proto.upvalues[i];
        if uv.name == Some(name) {
            *e = ExprDesc::init(ExpKind::Upval, i as i32);
            return;
        }
    }
    // Global: _ENV[name]
    // For the main chunk, _ENV is upvalue 0.
    *e = ExprDesc::init(ExpKind::Upval, 0);
    let mut key = ExprDesc::void();
    key.k = ExpKind::KStr;
    key.strval = Some(name);
    key.t = NO_JUMP;
    key.f = NO_JUMP;
    lcode::indexed(fs, e, &mut key);
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
    let ridx = fs.freereg;
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

    // TODO: parse_if_then_else test deferred — comparison codegen
    // needs go_if_true to handle ExpKind::Jmp properly.
}
