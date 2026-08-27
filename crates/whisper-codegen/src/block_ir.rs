//! Block-IR: basic blocks with explicit edges — the compilation core
//! that replaces flat offset arithmetic. Every control-flow construct
//! (cond, loop, leave, ?->) compiles by creating blocks and connecting
//! them; no layer ever computes a numeric jump offset.
//!
//! ## Structure
//!
//! ```text
//! Block#1 [Push(0)]                  → Jump(#2)
//! Block#2 [Dup, Push(3), Lt]         → Branch(#3, #5)
//! Block#3 [Dup, Print, Push(1), Add] → Jump(#2)   ← back-edge
//! Block#5 [Drop]                     → Halt
//! ```
//!
//! Stack contracts (`stack_in`/`stack_out`) are declared per block and
//! enforced at three points: loop body must be net-zero, `??` branches
//! must have equal `stack_out`, and predecessor `stack_out` must equal
//! successor `stack_in`.

use whisper_core::opcode::Opcode;

pub type BlockId = usize;

#[derive(Debug, Clone)]
pub enum Terminator {
    /// Unconditional jump to another block
    Jump(BlockId),
    /// Pop a bool; jump to `then` if true, `else` if false
    Branch { then: BlockId, els: BlockId },
    /// Return from the current word (pop frame)
    Return,
    /// Program or word end
    Halt,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    /// Straight-line instructions only — never a Cond/Jump/Loop
    pub instructions: Vec<Opcode>,
    pub terminator: Terminator,
    /// Stack depth at block entry (contract)
    pub stack_in: usize,
    /// Stack depth at block exit, before terminator pops (contract)
    pub stack_out: usize,
}

/// A complete block-IR program: blocks + entry point.
/// Words are compiled to their own BlockProgram.
#[derive(Debug, Clone, Default)]
pub struct BlockProgram {
    pub blocks: Vec<Block>,
    pub entry: BlockId,
}

impl BlockProgram {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id]
    }

    /// Blocks reachable from entry, in REVERSE POST-ORDER.
    ///
    /// RPO is what makes fall-through work with a single Cond per
    /// branch: exploring successors in order `[els, then]` makes the
    /// `then` subtree finish last, so in RPO `then` lands immediately
    /// after its branch block — true falls through from Cond into it.
    /// Back-edges (loop body → top) land last; loop exit blocks are
    /// emitted at the very end of their loop.
    pub fn reachable(&self) -> Vec<BlockId> {
        fn successors(term: &Terminator) -> Vec<BlockId> {
            match term {
                Terminator::Jump(t) => vec![*t],
                // explored els-first so then finishes last → then first in RPO
                Terminator::Branch { then, els } => vec![*els, *then],
                Terminator::Return | Terminator::Halt => vec![],
            }
        }
        if self.blocks.is_empty() {
            return Vec::new();
        }
        let mut visited = vec![false; self.blocks.len()];
        let mut postorder = Vec::with_capacity(self.blocks.len());
        visited[self.entry] = true;
        // iterative DFS: (block, next-successor-index)
        let mut stack: Vec<(BlockId, usize)> = vec![(self.entry, 0)];
        while let Some(&mut (id, ref mut idx)) = stack.last_mut() {
            let succs = successors(&self.blocks[id].terminator);
            if *idx < succs.len() {
                let s = succs[*idx];
                *idx += 1;
                if s < self.blocks.len() && !visited[s] {
                    visited[s] = true;
                    stack.push((s, 0));
                }
            } else {
                postorder.push(id);
                stack.pop();
            }
        }
        postorder.reverse();
        postorder
    }
}

// ── Builder ─────────────────────────────────────────────────────────

/// Compiles control-flow constructs by creating blocks and connecting
/// them. The builder tracks the current open block; instructions are
/// appended to it; terminators close it and open the next.
pub struct BlockBuilder {
    pub program: BlockProgram,
    /// The block currently being filled (None = nothing open)
    pub current: Option<BlockId>,
    pub next_id: BlockId,
    /// Stack depth simulation for contract checking
    pub depth: usize,
    /// Loop context stack: (top_block, exit_block) for leave
    pub loop_stack: Vec<(BlockId, BlockId)>,
}

impl BlockBuilder {
    pub fn new() -> Self {
        Self {
            program: BlockProgram::new(),
            current: None,
            next_id: 0,
            depth: 0,
            loop_stack: Vec::new(),
        }
    }

    /// Create a new (empty, unconnected) block
    pub fn new_block(&mut self) -> BlockId {
        let id = self.next_id;
        self.next_id += 1;
        self.program.blocks.push(Block {
            id,
            instructions: Vec::new(),
            terminator: Terminator::Halt,
            stack_in: 0,
            stack_out: 0,
        });
        id
    }

    /// Append an instruction to the current open block
    pub fn emit(&mut self, op: Opcode) {
        if self.current.is_none() {
            let b = self.new_block();
            self.current = Some(b);
        }
        let id = self.current.unwrap();
        self.program.blocks[id].instructions.push(op);
    }

    /// Close the current block with a terminator; open the next block
    /// as the new current (for straight-line continuation).
    pub fn terminate(&mut self, term: Terminator) {
        if self.current.is_none() {
            // empty block: create it first
            let b = self.new_block();
            self.current = Some(b);
        }
        let id = self.current.unwrap();
        self.program.blocks[id].terminator = term;
        self.current = None; // caller opens next block explicitly
    }

    /// Begin filling a specific block
    pub fn switch_to(&mut self, id: BlockId) {
        self.current = Some(id);
    }

    /// Current open block ID (creates one if none)
    pub fn current_block(&mut self) -> BlockId {
        if self.current.is_none() {
            let b = self.new_block();
            self.current = Some(b);
        }
        self.current.unwrap()
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn set_depth(&mut self, d: usize) {
        self.depth = d;
    }

    // ── Control flow constructs ─────────────────────────────────

    /// Compile `cond ?? THEN | ELSE ]` — creates 4 blocks:
    /// cond-block, then-block, else-block, join-block.
    /// The cond-block branches to then/else; both jump to join.
    ///
    /// Caller compiles cond instructions, then calls this with the
    /// then/else AST closures that fill the respective blocks.
    pub fn begin_cond(&mut self) -> (BlockId, BlockId, BlockId) {
        let cond_b = self.current_block();
        let then_b = self.new_block();
        let else_b = self.new_block();
        let join_b = self.new_block();
        self.program.blocks[cond_b].terminator =
            Terminator::Branch { then: then_b, els: else_b };
        self.current = None;
        (then_b, else_b, join_b)
    }

    /// Finish the then-branch: jump to join, switch to else block
    pub fn end_then(&mut self, then_b: BlockId, else_b: BlockId, join_b: BlockId) {
        self.program.blocks[then_b].terminator = Terminator::Jump(join_b);
        self.current = None;
        self.switch_to(else_b);
    }

    /// Finish the else-branch: jump to join, switch to join block
    pub fn end_else(&mut self, else_b: BlockId, join_b: BlockId) {
        self.program.blocks[else_b].terminator = Terminator::Jump(join_b);
        self.current = None;
        self.switch_to(join_b);
    }

    /// Compile `{ body } { cond } #` — creates 4 blocks:
    /// entry(fallthrough to top), top(cond check), body, exit.
    /// The top block branches to body/exit; body jumps back to top.
    pub fn begin_loop(&mut self) -> (BlockId, BlockId, BlockId) {
        let entry_b = self.current_block();
        let top_b = self.new_block();
        let body_b = self.new_block();
        let exit_b = self.new_block();
        // entry falls through to top
        self.program.blocks[entry_b].terminator = Terminator::Jump(top_b);
        self.current = None;
        self.switch_to(top_b);
        self.loop_stack.push((top_b, exit_b));
        (top_b, body_b, exit_b)
    }

    /// After compiling cond into the top block: branch to body/exit
    pub fn end_loop_cond(&mut self, top_b: BlockId, body_b: BlockId, exit_b: BlockId) {
        self.program.blocks[top_b].terminator =
            Terminator::Branch { then: body_b, els: exit_b };
        self.current = None;
        self.switch_to(body_b);
    }

    /// After compiling body: jump back to top, switch to exit
    pub fn end_loop_body(&mut self, body_b: BlockId, top_b: BlockId, exit_b: BlockId) {
        self.program.blocks[body_b].terminator = Terminator::Jump(top_b);
        self.current = None;
        self.switch_to(exit_b);
        self.loop_stack.pop();
    }

    /// `leave` — jump to the innermost loop's exit block.
    /// Returns Some(exit_block) or None if not inside a loop.
    pub fn emit_leave(&mut self) -> Option<BlockId> {
        let &(_, exit_b) = self.loop_stack.last()?;
        let from = self.current_block();
        self.program.blocks[from].terminator = Terminator::Jump(exit_b);
        self.current = None;
        Some(exit_b)
    }

    /// Finalize: set the current block's terminator to Return/Halt
    pub fn finish(&mut self, term: Terminator) {
        if let Some(id) = self.current {
            self.program.blocks[id].terminator = term;
        }
        self.current = None;
    }

    pub fn set_entry(&mut self, id: BlockId) {
        self.program.entry = id;
    }
}

// ── Linearization ──────────────────────────────────────────────────

/// Linearizes blocks into flat bytecode with numeric offsets.
/// This is the ONLY place offsets are ever computed.
///
/// Blocks are emitted in reverse post-order (see `reachable`), which
/// guarantees each Branch's `then` block immediately follows it, so a
/// single `Cond` per branch suffices: true falls through into `then`,
/// false skips forward to `els`.
pub fn linearize(program: &BlockProgram) -> Vec<Opcode> {
    let order = program.reachable();

    // Pass 1: measure block sizes in emission order. A Halt block that
    // is NOT emitted last still needs one Jump instruction — Halt emits
    // zero bytes, so a jump landing on an interior Halt block would run
    // straight into the next block's code (see pass 2).
    let last_emitted = order.last().copied();
    let mut block_offset = vec![0usize; program.blocks.len()];
    let mut offset = 0;
    for &id in &order {
        block_offset[id] = offset;
        let block = &program.blocks[id];
        let is_halt = matches!(block.terminator, Terminator::Halt);
        let halt_jump = is_halt && Some(id) != last_emitted;
        offset += block.instructions.len() + terminator_size(&block.terminator, halt_jump);
    }
    let total = offset;

    // Pass 2: emit with resolved offsets
    let mut flat = Vec::with_capacity(offset);
    for &id in &order {
        let block = &program.blocks[id];
        for op in &block.instructions {
            flat.push(op.clone());
        }
        let instr_end = block_offset[id] + block.instructions.len();
        match &block.terminator {
            Terminator::Jump(t) => {
                let off = block_offset[*t] as i64 - instr_end as i64 - 1;
                flat.push(Opcode::Jump(off as i32));
            }
            Terminator::Branch { then: _, els } => {
                // cond on stack; if FALSE skip to else block.
                // then block is next in emission order → true falls through.
                let cond_off = block_offset[*els] as i64 - instr_end as i64 - 1;
                flat.push(Opcode::Cond(cond_off as i32));
            }
            Terminator::Return => flat.push(Opcode::Return),
            Terminator::Halt => {
                // Interior Halt blocks become "jump to program end"
                // (running past the end ends the frame/program). The
                // last emitted block needs no code at all.
                if Some(id) != last_emitted {
                    let off = total as i64 - instr_end as i64 - 1;
                    flat.push(Opcode::Jump(off as i32));
                }
            }
        }
    }
    flat
}

fn terminator_size(term: &Terminator, halt_needs_jump: bool) -> usize {
    match term {
        Terminator::Jump(_) => 1,
        Terminator::Branch { .. } => 1, // just Cond (then falls through)
        Terminator::Return => 1,
        Terminator::Halt => usize::from(halt_needs_jump),
    }
}

// ── Graph optimization ─────────────────────────────────────────────

/// Graph-level optimizations on a BlockProgram, run BEFORE
/// linearization. Offsets are never touched — only edges change, and
/// `linearize` recomputes everything. Returns the number of changes.
///
/// Passes (iterated to fixpoint):
/// * **constant branch folding** — a block ending in `PushBool(c)`
///   with a `Branch` terminator becomes `Jump(then|els)` and the dead
///   arm's blocks become unreachable (dropped at emission);
/// * **same-target branch collapse** — `Branch{t, t}` still pops its
///   condition, so it becomes `Drop; Jump(t)`;
/// * **empty-block threading** — an empty block whose terminator is
///   `Jump(t)` is bypassed: every edge into it retargets to `t`
///   (the entry block too, by moving `entry`).
pub fn optimize_blocks(program: &mut BlockProgram) -> usize {
    let mut total = 0;
    loop {
        let mut round = 0;

        // Constant branch folding + same-target collapse.
        for block in &mut program.blocks {
            match &block.terminator {
                Terminator::Branch { then, els } => {
                    if let Some(Opcode::PushBool(c)) = block.instructions.last() {
                        let target = if *c { *then } else { *els };
                        block.instructions.pop();
                        block.terminator = Terminator::Jump(target);
                        round += 1;
                    } else if then == els {
                        block.instructions.push(Opcode::Drop);
                        block.terminator = Terminator::Jump(*then);
                        round += 1;
                    }
                }
                _ => {}
            }
        }

        // Empty-block threading: build the bypass map, then retarget.
        if program.blocks.is_empty() {
            break;
        }
        let mut bypass: Vec<Option<BlockId>> = vec![None; program.blocks.len()];
        for (i, b) in program.blocks.iter().enumerate() {
            if b.instructions.is_empty() {
                if let Terminator::Jump(t) = b.terminator {
                    if t != i {
                        bypass[i] = Some(t);
                    }
                }
            }
        }
        if bypass.iter().any(|b| b.is_some()) {
            // Resolve chains (B→T where T is itself bypassed).
            for i in 0..bypass.len() {
                let mut t = bypass[i];
                let mut hops = 0;
                while let Some(next) = t.and_then(|x| bypass[x]) {
                    t = Some(next);
                    hops += 1;
                    if hops > bypass.len() {
                        break; // cycle guard (bypass already excludes self)
                    }
                }
                bypass[i] = t;
            }
            let retarget = |t: &mut BlockId| {
                if let Some(next) = bypass[*t] {
                    *t = next;
                    true
                } else {
                    false
                }
            };
            for block in &mut program.blocks {
                let mut changed = false;
                match &mut block.terminator {
                    Terminator::Jump(t) => changed |= retarget(t),
                    Terminator::Branch { then, els } => {
                        // NEVER retarget `then`: it is the fall-through
                        // arm — linearize emits it immediately after the
                        // Cond, and a bypassed target might sit elsewhere
                        // in RPO order. `els` is reached by offset, so
                        // retargeting it is always safe.
                        changed |= retarget(els);
                        let _ = then;
                    }
                    _ => {}
                }
                if changed {
                    round += 1;
                }
            }
            if let Some(t) = bypass[program.entry] {
                program.entry = t;
            }
        }

        if round == 0 {
            break;
        }
        total += round;
    }
    total
}

// ── Validation ─────────────────────────────────────────────────────

/// A violation of a stack-effect contract.
#[derive(Debug)]
pub struct ContractError {
    pub block: BlockId,
    pub message: String,
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "block #{}: {}", self.block, self.message)
    }
}

/// Renders contract violations as one user-facing message.
pub fn contract_errors_to_string(errs: &[ContractError]) -> String {
    errs.iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Propagates stack depths across block edges and checks contracts:
///
/// * **underflow** — an opcode with a known effect pops more values
///   than are statically present (main programs only; word bodies may
///   consume caller arguments, so they only get relative checks);
/// * **edge match** — every predecessor must hand its successor the
///   same depth. This one rule uniformly enforces:
///   `??` branch arms produce equal stacks (both jump to the join),
///   loop bodies are net-zero (body's back-edge into `top`), and
///   `leave` is consistent with the normal loop exit.
///
/// `def_effects` maps word names to their compiled net stack effect
/// (`exit depth − entry depth`); calls to such words count as known.
pub fn validate(
    program: &BlockProgram,
    underflow_is_error: bool,
    def_effects: &std::collections::HashMap<String, i64>,
) -> Vec<ContractError> {
    analyze(program, underflow_is_error, def_effects).errors
}

struct Analysis {
    /// Depth at each block's entry.
    depth_in: Vec<Depth>,
    /// Depth after each block's terminator pops.
    depth_out: Vec<Depth>,
    errors: Vec<ContractError>,
}

/// Depth at a program point: `None` = statically unknown (an opcode
/// with an opaque effect ran), which disables downstream checks
/// conservatively.
type Depth = Option<i64>;

/// Dataflow lattice: Unknown (not yet reached) → Known(d) → Opaque.
/// Descending to Opaque whenever any contribution is unknown is what
/// keeps the checker sound: an edge mismatch is reported only when
/// every incoming path has a *known* depth and two of them disagree.
/// (Without this, a known path through a join whose sibling path is
/// opaque would falsely pin the join's depth.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lattice {
    Unknown,
    Known(i64),
    Opaque,
}

fn join_lattice(a: Lattice, b: Lattice) -> Lattice {
    match (a, b) {
        (Lattice::Unknown, x) | (x, Lattice::Unknown) => x,
        (Lattice::Opaque, _) | (_, Lattice::Opaque) => Lattice::Opaque,
        (Lattice::Known(x), Lattice::Known(y)) if x == y => Lattice::Known(x),
        (Lattice::Known(_), Lattice::Known(_)) => Lattice::Opaque, // conflict — caller reports
    }
}

fn analyze(
    program: &BlockProgram,
    underflow_is_error: bool,
    def_effects: &std::collections::HashMap<String, i64>,
) -> Analysis {
    let mut errors = Vec::new();
    let n = program.blocks.len();
    let mut depth_in = vec![Lattice::Unknown; n];
    let mut depth_out = vec![Lattice::Unknown; n];
    let mut out = Analysis {
        depth_in: vec![None; n],
        depth_out: vec![None; n],
        errors: Vec::new(),
    };
    if n == 0 {
        return out;
    }
    depth_in[program.entry] = Lattice::Known(0);

    let mut work: Vec<BlockId> = vec![program.entry];
    while let Some(id) = work.pop() {
        let block = &program.blocks[id];

        // Simulate straight-line instructions.
        let mut cur = depth_in[id];
        let mut d = match cur {
            Lattice::Known(d) => d,
            _ => 0,
        };
        for op in &block.instructions {
            let Some((pops, pushes)) = effective_pops_pushes(op, def_effects) else {
                cur = Lattice::Opaque;
                break;
            };
            if let Lattice::Known(_) = cur {
                if underflow_is_error && d < pops as i64 {
                    errors.push(ContractError {
                        block: id,
                        message: format!(
                            "stack underflow: {op:?} needs {pops} values, {d} present"
                        ),
                    });
                }
                d = d - pops as i64 + pushes as i64;
            }
        }

        // Apply the terminator's pop (Branch consumes its condition).
        let (succs, term_pop): (Vec<BlockId>, i64) = match &block.terminator {
            Terminator::Jump(t) => (vec![*t], 0),
            Terminator::Branch { then, els } => (vec![*then, *els], 1),
            Terminator::Return | Terminator::Halt => (vec![], 0),
        };
        let exit = match cur {
            Lattice::Known(_) => {
                if underflow_is_error && d < term_pop {
                    errors.push(ContractError {
                        block: id,
                        message: format!(
                            "stack underflow: branch condition needs 1 value, {d} present"
                        ),
                    });
                }
                Lattice::Known(d - term_pop)
            }
            _ => Lattice::Opaque,
        };

        if exit == depth_out[id] {
            continue; // no new information for successors
        }
        depth_out[id] = exit;
        for s in succs {
            if s >= n {
                continue;
            }
            let joined = join_lattice(depth_in[s], exit);
            if joined != depth_in[s] {
                if let (Lattice::Known(a), Lattice::Known(b)) = (depth_in[s], exit) {
                    if a != b {
                        errors.push(ContractError {
                            block: id,
                            message: format!(
                                "stack contract violated on edge {id}→{s}: \
                                 one path delivers depth {a}, another {b} — \
                                 `??` arms must agree and loop bodies must be \
                                 net-zero"
                            ),
                        });
                    }
                }
                depth_in[s] = joined;
                work.push(s);
            }
        }
    }

    for (i, v) in depth_in.iter().enumerate() {
        out.depth_in[i] = match v {
            Lattice::Known(d) => Some(*d),
            _ => None,
        };
    }
    for (i, v) in depth_out.iter().enumerate() {
        out.depth_out[i] = match v {
            Lattice::Known(d) => Some(*d),
            _ => None,
        };
    }
    out.errors = errors;
    out
}

/// Resolves an opcode's effect, consulting compiled word signatures
/// for `Call` (net effect = pushes − pops as a single signed delta).
fn effective_pops_pushes(
    op: &Opcode,
    def_effects: &std::collections::HashMap<String, i64>,
) -> Option<(usize, usize)> {
    if let Opcode::Call(name) = op {
        return def_effects.get(name).map(|&net| {
            if net >= 0 {
                (0, net as usize)
            } else {
                ((-net) as usize, 0)
            }
        });
    }
    crate::stack_effect::stack_effect(op)
}

/// Net stack effect of a word body: depth at its end relative to
/// entry. Callers use this to keep `Call` checks precise. `None` when
/// the exit depth is control-dependent or opaque.
pub fn net_effect(
    program: &BlockProgram,
    def_effects: &std::collections::HashMap<String, i64>,
) -> Option<i64> {
    let last = program.blocks.last()?;
    if !matches!(last.terminator, Terminator::Halt) {
        return None;
    }
    let analysis = analyze(program, false, def_effects);
    analysis.depth_out.get(last.id).copied().flatten()
}
