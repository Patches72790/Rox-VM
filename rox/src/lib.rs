mod chunk;
mod compiler;
mod error;
mod opcode;
mod precedence;
mod raw_stack;
mod run;
mod scanner;
mod token;
mod value;
mod vm;

pub use chunk::*;
pub use compiler::*;
pub use error::*;
pub use opcode::OpCode;
pub use precedence::Precedence;
pub use raw_stack::RawStack as Stack;
pub use run::*;
pub use scanner::Scanner;
pub use token::*;
pub use value::*;
pub use vm::*;

pub static DEBUG_MODE: bool = false;
pub const STACK_MAX: usize = 256;
