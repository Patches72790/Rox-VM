use crate::frontend::{Local, Locals, LOCALS_COUNT};
use crate::opcode::VariableOp;
use crate::{
    Chunk, ObjectType, OpCode, Precedence, RoxNumber, RoxObject, RoxString, Token, TokenType,
    Value, DEBUG_MODE,
};
use std::cell::RefCell;
use std::iter::Peekable;
use std::rc::Rc;
use std::slice::Iter;

pub struct Compiler<'a> {
    chunk: Rc<RefCell<Chunk>>,
    tokens: RefCell<Peekable<Iter<'a, Token>>>,
    previous: RefCell<Option<&'a Token>>,
    current: RefCell<Option<&'a Token>>,
    pub had_error: RefCell<bool>,
    pub panic_mode: RefCell<bool>,

    locals: RefCell<Locals>,
    scope_depth: RefCell<usize>,
}

type ParseFn<'a> = Box<dyn FnOnce(bool) + 'a>;

struct ParseRule<'a> {
    precedence: Precedence,
    infix_fn: Option<ParseFn<'a>>,
    prefix_fn: Option<ParseFn<'a>>,
}

impl<'a> Compiler<'a> {
    pub fn new(
        chunk: Rc<RefCell<Chunk>>,
        tokens: RefCell<Peekable<Iter<'a, Token>>>,
    ) -> Compiler<'a> {
        Compiler {
            chunk,
            tokens,
            had_error: RefCell::new(false),
            panic_mode: RefCell::new(false),
            previous: RefCell::new(None),
            current: RefCell::new(None),
            scope_depth: RefCell::new(0),
            locals: RefCell::new(Locals::new()),
        }
    }

    fn get_rule(&'a self, token: &'a Token) -> ParseRule {
        let t_type = &token.token_type;
        let line = token.line;

        match t_type {
            TokenType::And => ParseRule {
                precedence: Precedence::PrecAnd,
                infix_fn: Some(Box::new(|can_assign| self.and_(can_assign))),
                prefix_fn: None,
            },
            TokenType::Or => ParseRule {
                precedence: Precedence::PrecOr,
                infix_fn: Some(Box::new(|can_assign| self.or(can_assign))),
                prefix_fn: None,
            },
            TokenType::Plus => ParseRule {
                precedence: Precedence::PrecTerm,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
                prefix_fn: None,
            },
            TokenType::Minus => ParseRule {
                precedence: Precedence::PrecTerm,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
                prefix_fn: Some(Box::new(|can_assign| self.unary(can_assign))),
            },
            TokenType::Star => ParseRule {
                precedence: Precedence::PrecFactor,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::Slash => ParseRule {
                precedence: Precedence::PrecFactor,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::Number(num) => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(move |can_assign| {
                    self.number(*num, line, can_assign)
                })),
                infix_fn: None,
            },
            TokenType::True => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(|can_assign| self.literal(can_assign))),
                infix_fn: None,
            },
            TokenType::False => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(|can_assign| self.literal(can_assign))),
                infix_fn: None,
            },
            TokenType::Nil => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(|can_assign| self.literal(can_assign))),
                infix_fn: None,
            },
            TokenType::Bang => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(|can_assign| self.unary(can_assign))),
                infix_fn: None,
            },
            TokenType::BangEqual => ParseRule {
                precedence: Precedence::PrecEquality,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::EqualEqual => ParseRule {
                precedence: Precedence::PrecEquality,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::Greater => ParseRule {
                precedence: Precedence::PrecComparison,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::GreaterEqual => ParseRule {
                precedence: Precedence::PrecComparison,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::Less => ParseRule {
                precedence: Precedence::PrecComparison,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::LessEqual => ParseRule {
                precedence: Precedence::PrecComparison,
                prefix_fn: None,
                infix_fn: Some(Box::new(|can_assign| self.binary(can_assign))),
            },
            TokenType::LeftParen => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(|can_assign| self.grouping(can_assign))),
                infix_fn: None,
            },
            TokenType::RightParen => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: None,
                infix_fn: None,
            },
            TokenType::Semicolon => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: None,
                infix_fn: None,
            },
            TokenType::Identifier(id) => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(move |can_assign| {
                    self.variable(id, line, can_assign)
                })),
                infix_fn: None,
            },
            TokenType::StringLiteral(str) => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: Some(Box::new(move |can_assign| {
                    self.string(str, line, can_assign)
                })),
                infix_fn: None,
            },
            TokenType::EOF => ParseRule {
                precedence: Precedence::PrecNone,
                prefix_fn: None,
                infix_fn: None,
            },
            _ => todo!("Unimplemented token type: {:?}", t_type),
        }
    }

    fn advance(&self) {
        // set previous to current token
        let current_tok = *self.current.borrow();
        *(self.previous.borrow_mut()) = current_tok;

        // set current to the next token in scanner tokens
        // until no error token is found
        loop {
            let next_token = match self.tokens.borrow_mut().next() {
                Some(tok) => tok,
                None => return, //  panic!("Error getting next token in advance!"),
            };

            if DEBUG_MODE {
                println!("Advanced to Token: {}", next_token);
            }

            *(self.current.borrow_mut()) = Some(next_token);

            if let TokenType::Error(msg) = &next_token.token_type {
                self.error_at_current_token(msg);
            } else {
                break;
            }
        }
    }

    /// A conditional wrapper around advance that checks that
    /// the current token is of type t_type.
    fn match_token(&self, t_type: TokenType) -> bool {
        if !self.check_token(t_type) {
            return false;
        }
        self.advance();
        true
    }

    /// Helper function to check that the current token's
    /// type is equal to t_type.
    fn check_token(&self, t_type: TokenType) -> bool {
        self.current
            .borrow()
            .expect("Error borrowing next token in parser check!")
            .token_type
            == t_type
    }

    fn consume(&self, t_type: TokenType, message: &str) {
        let current_tok = self
            .current
            .borrow()
            .expect("Error consuming current token!");
        if current_tok.token_type == t_type {
            if DEBUG_MODE {
                println!("Consuming token {}", current_tok);
            }
            self.advance();
            return;
        }

        self.error_at_current_token(message);
    }

    fn error_at_current_token(&self, message: &str) {
        self.error_at(
            self.current
                .borrow()
                .expect("Error borrowing current token"),
            message,
        );
    }

    fn error(&self, message: &str) {
        self.error_at(
            self.previous
                .borrow()
                .expect("Error borrowing previous token"),
            message,
        );
    }

    fn error_at(&self, token: &Token, message: &str) {
        // if already in panic, stop parser
        if *self.panic_mode.borrow() {
            return;
        }

        *self.panic_mode.borrow_mut() = true;

        eprintln!(
            "Error at [{}, {}] with message: {}",
            token.line, token.column, message
        );
        *self.had_error.borrow_mut() = true;
    }

    fn synchronize(&'a self) {
        *self.panic_mode.borrow_mut() = false;
        let mut current_token_type = &self
            .current
            .borrow()
            .expect("Error borrowing current token while synchronizing compiler")
            .token_type;

        while *current_token_type != TokenType::EOF {
            if self
                .previous
                .borrow()
                .expect("Error borrowing previous token while synchronizing compiler")
                .token_type
                == TokenType::Semicolon
            {
                return;
            }

            match current_token_type {
                TokenType::Class
                | TokenType::Fun
                | TokenType::Var
                | TokenType::For
                | TokenType::If
                | TokenType::While
                | TokenType::Print
                | TokenType::Return => return,
                _ => (),
            }

            // indiscriminately advance depending on token type until end of statement is found
            self.advance();
            current_token_type = &self
                .current
                .borrow()
                .expect("Error borrowing current token while synchronizing compiler")
                .token_type;
        }
    }

    fn expression(&'a self) {
        self.parse(&Precedence::PrecAssign);
    }

    fn declaration(&'a self) {
        if self.match_token(TokenType::Var) {
            self.var_declaration();
        } else {
            self.statement();
        }
        if *self.panic_mode.borrow() {
            self.synchronize();
        }
    }

    fn var_declaration(&'a self) {
        let index = self.parse_variable("Expect variable name.");

        if self.match_token(TokenType::Equal) {
            self.expression();
        } else {
            self.emit_byte(OpCode::OpNil);
        }

        self.consume(
            TokenType::Semicolon,
            "Expect ';' after variable declaration.",
        );

        self.define_variable(index);
    }

    fn declare_variable(&'a self) {
        // for globals
        if *self.scope_depth.borrow() == 0 {
            return;
        }

        let token = &*self
            .previous
            .borrow()
            .expect("Error borrowing previous token when declaring local variable.");

        let is_doubly_declared = self
            .locals
            .borrow()
            .local_is_doubly_declared(token, *self.scope_depth.borrow());

        if is_doubly_declared {
            self.error("Already a variable with this name in scope.");
            return;
        }

        self.add_local(token);
    }

    fn add_local(&'a self, token: &Token) {
        let locals_count = self.locals.borrow().size();
        if locals_count == LOCALS_COUNT {
            self.error("Too many local variables in function.");
            return;
        }

        self.locals
            .borrow_mut()
            .add_local(token, *self.scope_depth.borrow());
    }

    fn define_variable(&'a self, index: usize) {
        let scope_depth = *self.scope_depth.borrow();
        if scope_depth > 0 {
            self.locals.borrow_mut().initialize_variable(scope_depth);
            return;
        }

        self.emit_byte(OpCode::OpDefineGlobal(index));
    }

    fn statement(&'a self) {
        if self.match_token(TokenType::Print) {
            self.print_statement();
        } else if self.match_token(TokenType::For) {
            self.for_statement();
        } else if self.match_token(TokenType::If) {
            self.if_statement();
        } else if self.match_token(TokenType::While) {
            self.while_statement();
        } else if self.match_token(TokenType::LeftBrace) {
            self.begin_scope();
            self.block();
            self.end_scope();
        } else {
            self.expression_statement();
        }
    }

    fn print_statement(&'a self) {
        self.expression();
        self.consume(TokenType::Semicolon, "Expected ';' after value.");
        self.emit_byte(OpCode::OpPrint);
    }

    fn expression_statement(&'a self) {
        self.expression();
        self.consume(
            TokenType::Semicolon,
            "Expected ';' after expression statement.",
        );
        self.emit_byte(OpCode::OpPop);
    }

    fn for_statement(&'a self) {
        self.begin_scope();
        self.consume(TokenType::LeftParen, "Expect '(' after 'for'.");

        // compile initialization statement
        if self.match_token(TokenType::Semicolon) {
            // no initializer
        } else if self.match_token(TokenType::Var) {
            self.var_declaration();
        } else {
            self.expression_statement();
        }

        let mut loop_start = self.chunk.borrow().count();

        // compile conditional statement
        let mut exit_jump = None;
        if !self.match_token(TokenType::Semicolon) {
            self.expression();
            self.consume(TokenType::Semicolon, "Expect ';' after loop condition.");

            exit_jump = Some(self.emit_jump(OpCode::OpJumpIfFalse(None)));
            self.emit_byte(OpCode::OpPop);
        }

        // compile increment statement
        if !self.match_token(TokenType::RightParen) {
            let body_jump = self.emit_jump(OpCode::OpJump(None));
            let incr_start = self.chunk.borrow().count();

            self.expression();
            self.emit_byte(OpCode::OpPop);
            self.consume(TokenType::RightParen, "Expect ')' after for clauses.");

            self.emit_loop(loop_start);
            loop_start = incr_start;
            self.patch_jump(body_jump, OpCode::OpJump(None));
        }

        self.statement();
        self.emit_loop(loop_start);

        // compile code to quit for loop early when condition is false
        if let Some(exit_jump_offset) = exit_jump {
            self.patch_jump(exit_jump_offset, OpCode::OpJumpIfFalse(None));
            self.emit_byte(OpCode::OpPop);
        }

        self.end_scope();
    }

    fn while_statement(&'a self) {
        let loop_start = self.chunk.borrow().count();

        self.consume(TokenType::LeftParen, "Expect '(' after 'while'.");
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after condition.");

        let exit_jump = self.emit_jump(OpCode::OpJumpIfFalse(None));
        self.emit_jump(OpCode::OpPop);
        self.statement();
        self.emit_loop(loop_start);

        self.patch_jump(exit_jump, OpCode::OpJumpIfFalse(None));
        self.emit_byte(OpCode::OpPop);
    }

    fn if_statement(&'a self) {
        self.consume(TokenType::LeftParen, "Expect '(' after 'if'.");
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after condition.");

        let then_jump = self.emit_jump(OpCode::OpJumpIfFalse(None));
        self.emit_byte(OpCode::OpPop);
        self.statement();

        let else_jump = self.emit_jump(OpCode::OpJump(None));

        self.patch_jump(then_jump, OpCode::OpJumpIfFalse(None));
        self.emit_byte(OpCode::OpPop);

        if self.match_token(TokenType::Else) {
            self.statement();
        }
        self.patch_jump(else_jump, OpCode::OpJump(None));
    }

    fn emit_jump(&'a self, instruction: OpCode) -> usize {
        self.emit_byte(instruction);
        self.chunk.borrow().count() - 1
    }

    fn patch_jump(&'a self, offset: usize, opcode: OpCode) {
        let jump = self.chunk.borrow().count() - offset - 1;

        // patch in the jump offset from the jump opcode to past the then clause
        match opcode {
            OpCode::OpJumpIfFalse(_) => {
                self.chunk.borrow_mut().code[offset] = OpCode::OpJumpIfFalse(Some(jump))
            }
            OpCode::OpJump(_) => self.chunk.borrow_mut().code[offset] = OpCode::OpJump(Some(jump)),
            _ => (),
        }
    }

    fn block(&'a self) {
        while !self.check_token(TokenType::RightBrace) && !self.check_token(TokenType::EOF) {
            self.declaration();
        }

        self.consume(TokenType::RightBrace, "Expect '}' after block.");
    }

    fn begin_scope(&'a self) {
        *self.scope_depth.borrow_mut() += 1;
    }

    fn end_scope(&'a self) {
        *self.scope_depth.borrow_mut() -= 1;
        let scope_depth = *self.scope_depth.borrow();

        let num_removed = self.locals.borrow_mut().remove_locals(scope_depth);

        for _ in 0..num_removed {
            self.emit_byte(OpCode::OpPop);
        }
    }

    fn and_(&'a self, _can_assign: bool) {
        let end_jump = self.emit_jump(OpCode::OpJumpIfFalse(None));

        self.emit_byte(OpCode::OpPop);
        self.parse(&Precedence::PrecAnd);

        self.patch_jump(end_jump, OpCode::OpJumpIfFalse(None));
    }

    fn or(&'a self, _can_assign: bool) {
        let else_jump = self.emit_jump(OpCode::OpJumpIfFalse(None));
        let end_jump = self.emit_jump(OpCode::OpJump(None));

        self.patch_jump(else_jump, OpCode::OpJumpIfFalse(None));
        self.emit_byte(OpCode::OpPop);

        self.parse(&Precedence::PrecOr);
        self.patch_jump(end_jump, OpCode::OpJump(None));
    }

    fn number(&'a self, num: RoxNumber, line: usize, _can_assign: bool) {
        self.emit_constant(Value::Number(num), line);
    }

    /// Writes a constant value to the chunk, bypassing
    /// emit_byte since the Chunk already has a convenience
    /// function for such a task.
    fn emit_constant(&self, value: Value, line: usize) {
        self.chunk.borrow_mut().add_constant(value, line);
    }

    fn emit_identifier_constant(
        &self,
        string_value: &Rc<RoxString>,
        line: usize,
        variable_op: VariableOp,
    ) -> usize {
        // need to write string to constants array in chunk
        self.chunk
            .borrow_mut()
            .add_identifier_constant(string_value, line, variable_op)
    }

    fn grouping(&'a self, _can_assign: bool) {
        self.expression();
        self.consume(TokenType::RightParen, "Expect ')' after expression.");
    }

    fn string(&'a self, string: &Rc<RoxString>, line: usize, _can_assign: bool) {
        let new_rox_object =
            RoxObject::new(ObjectType::ObjString(RoxString::new(&Rc::clone(string))));
        self.emit_constant(Value::Object(new_rox_object), line);
    }

    fn variable(&'a self, id: &Rc<RoxString>, line: usize, can_assign: bool) {
        let (is_initialized, is_local_id) = self.locals.borrow().resolve_local(id);

        if !is_initialized {
            self.error("Can't read local variable in its own initializer.");
        }

        // locals live on the stack at runtime
        if let Some(local_idx) = is_local_id {
            if can_assign && self.match_token(TokenType::Equal) {
                self.expression();
                self.emit_byte(OpCode::OpSetLocal(local_idx));
            } else {
                self.emit_byte(OpCode::OpGetLocal(local_idx));
            }
        } else {
            // globals live in globals list
            if can_assign && self.match_token(TokenType::Equal) {
                self.expression();
                self.chunk
                    .borrow_mut()
                    .add_identifier_constant(id, line, VariableOp::SetGlobal);
            } else {
                self.chunk
                    .borrow_mut()
                    .add_identifier_constant(id, line, VariableOp::GetGlobal);
            }
        }
    }

    fn literal(&'a self, _can_assign: bool) {
        match self
            .previous
            .borrow()
            .expect("Error borrowing previous token in literal")
            .token_type
        {
            TokenType::True => self.emit_byte(OpCode::OpTrue),
            TokenType::False => self.emit_byte(OpCode::OpFalse),
            TokenType::Nil => self.emit_byte(OpCode::OpNil),
            _ => (), // never will be here because literal only used for these three types
        }
    }

    fn unary(&'a self, _can_assign: bool) {
        // find type
        let operator_type = self
            .previous
            .borrow()
            .expect("Error borrowing previous token in unary");

        // compile operand
        self.parse(&Precedence::PrecUnary);

        // emit operator opcode
        match operator_type.token_type {
            TokenType::Minus => self.emit_byte(OpCode::OpNegate),
            TokenType::Bang => self.emit_byte(OpCode::OpNot),
            _ => panic!(
                "Error parsing unary expression. Unexpected token type: {}",
                operator_type
            ),
        }
    }

    fn binary(&'a self, _can_assign: bool) {
        let operator_type = self
            .previous
            .borrow()
            .expect("Error borrowing previous token in binary");

        // get parse rule
        let rule = self.get_rule(operator_type);

        // parse rule with next highest precedence (term -> factor, factor -> unary)
        self.parse(rule.precedence.get_next());

        // emit opcode for token type
        match operator_type.token_type {
            TokenType::Plus => self.emit_byte(OpCode::OpAdd),
            TokenType::Minus => self.emit_byte(OpCode::OpSubtract),
            TokenType::Star => self.emit_byte(OpCode::OpMultiply),
            TokenType::Slash => self.emit_byte(OpCode::OpDivide),
            TokenType::BangEqual => self.emit_bytes(OpCode::OpEqual, OpCode::OpNot),
            TokenType::EqualEqual => self.emit_byte(OpCode::OpEqual),
            TokenType::Greater => self.emit_byte(OpCode::OpGreater),
            TokenType::GreaterEqual => self.emit_bytes(OpCode::OpLess, OpCode::OpNot), // (a >= b) == !(a < b)
            TokenType::Less => self.emit_byte(OpCode::OpLess),
            TokenType::LessEqual => self.emit_bytes(OpCode::OpGreater, OpCode::OpNot), // (a <= b) == !(a > b)
            _ => panic!(
                "Error parsing binary expression. Unexpected token type: {}",
                operator_type
            ),
        }
    }

    fn emit_loop(&self, loop_start: usize) {
        let offset = self.chunk.borrow().count() - loop_start + 1;
        if offset > u16::MAX.into() {
            self.error("Loop body too large");
        }

        self.emit_byte(OpCode::OpLoop(offset));
    }

    fn emit_bytes(&self, byte1: OpCode, byte2: OpCode) {
        self.emit_byte(byte1);
        self.emit_byte(byte2);
    }

    fn emit_byte(&self, byte: OpCode) {
        let line = self
            .previous
            .borrow()
            .expect("Error borrowing previous token in emit byte")
            .line;
        self.chunk.borrow_mut().write_chunk(byte, line);
    }

    fn emit_return(&self) {
        self.emit_byte(OpCode::OpReturn(0));
    }

    fn end_compiler(&self) {
        self.emit_return();
    }

    fn parse(&'a self, precedence: &Precedence) {
        // advance cursor
        self.advance();

        let ParseRule { prefix_fn, .. } = self.get_rule(
            self.previous
                .borrow()
                .expect("Error borrowing previous token in parse"),
        );

        let can_assign = precedence <= &Precedence::PrecAssign;

        // call prefix parsing function if present
        if let Some(p_fn) = prefix_fn {
            p_fn(can_assign);
        } else if self.previous.borrow().unwrap().token_type == TokenType::EOF {
            return;
        } else {
            self.error(&format!(
                "No prefix function parsed for precedence {}.",
                precedence
            ));
            return;
        }

        // check that current precedence is less than current_token's precedence
        while precedence <= &self.get_rule(self.current.borrow().unwrap()).precedence {
            // advance cursor and execute infix parsing function
            self.advance();

            let ParseRule { infix_fn, .. } = self.get_rule(
                self.previous
                    .borrow()
                    .expect("Error borrowing previous in parse"),
            );

            if let Some(in_fn) = infix_fn {
                in_fn(can_assign);
            } else if self.previous.borrow().unwrap().token_type == TokenType::EOF {
                return;
            } else {
                self.error("No infix function parsed.");
                return;
            }

            if can_assign && self.match_token(TokenType::Equal) {
                self.error("Invalid assignment target.");
            }
        }
    }

    fn parse_variable(&'a self, msg: &str) -> usize {
        // TODO -- how to make parse variable work here without consuming blank ID?
        self.consume(TokenType::Identifier(Rc::new(RoxString::new(""))), msg);

        let previous = self
            .previous
            .borrow()
            .expect("Error borrowing previous token when parsing variable.");
        let previous_token_value = match &previous.token_type {
            TokenType::Identifier(str) => str,
            _ => panic!(
                "Error did not find identifier when parsing previous token for variable {}",
                previous
            ),
        };

        self.declare_variable();
        // don't add a local and a global below
        if *self.scope_depth.borrow() > 0 {
            return 0;
        }

        self.emit_identifier_constant(previous_token_value, previous.line, VariableOp::Define)
    }

    pub fn compile(&'a self) -> bool {
        // prime pump with token to parse
        self.advance();

        // parse sequence of declarations and statements
        while !self.match_token(TokenType::EOF) {
            self.declaration();
        }

        // emit final byte code
        self.end_compiler();

        !*self.had_error.borrow()
    }
}
