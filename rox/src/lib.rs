mod chunk;
mod compiler;
mod error;
mod frontend;
mod hashtable;
mod object;
mod object_list;
mod opcode;
mod precedence;
mod raw_stack;
mod run;
mod scanner;
mod token;
mod types;
mod value;
mod vm;

pub use chunk::*;
pub use compiler::*;
pub use error::*;
pub use hashtable::RoxMap;
pub use hashtable::Table;
pub use object::*;
pub use object_list::ObjectList;
pub use opcode::OpCode;
pub use precedence::Precedence;
pub use raw_stack::RawStack as Stack;
pub use run::*;
pub use scanner::Scanner;
pub use token::*;
pub use types::*;
pub use value::*;
pub use vm::*;

pub static DEBUG_MODE: bool = true;
pub const STACK_MAX: usize = 256;
