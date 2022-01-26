use crate::Token;
use std::cell::RefCell;

pub struct Compiler {
    func_table: RefCell<[Box<dyn FnMut()>; 2]>,
}

fn some_func() {
    println!("I'm a compiler function!");
}

fn other_func() {
    println!("I'm another compiler function!");
}

impl Compiler {
    pub fn new() -> Compiler {
        Compiler {
            func_table: RefCell::new([Box::new(some_func), Box::new(other_func)]),
        }
    }

    pub fn compile(&self, token: &Token) {
        println!("Compiler has a token: {}", token);
    }
}
